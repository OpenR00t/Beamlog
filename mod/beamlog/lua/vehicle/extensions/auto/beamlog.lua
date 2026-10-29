-- SPDX-License-Identifier: AGPL-3.0-or-later
-- Copyright (C) 2026 The Beamlog contributors
--
-- Beamlog recorder: records the vehicle being driven and writes MoTeC i2 files
-- (beamlog/<date>_<time>_<vehicle>.ld and .ldx in the user folder) when a run
-- ends. Controlled from the Beamlog UI app, or from the vehicle Lua console:
-- extensions.beamlog.start(), stop(), split(), lap(), setRate(hz), setAuto(bool).
--
-- Samples are taken on the physics step at a fixed rate (20, 50, 100 or 200
-- Hz; MoTeC wants every channel at a fixed rate). The game only calls an
-- extension's onPhysicsStep after enablePhysicsStepHook(); should that not
-- happen, samples are taken per graphics frame instead. They're kept in memory, already
-- in MoTeC's units and signs, as 32-bit integers. When a run ends the file is
-- written a channel per frame, so saving doesn't stall the game.
--
-- A run starts when the car moves (with auto on) or on start(), and ends on
-- stop(), split() (which starts the next one at once), a vehicle reset,
-- leaving or removing the car, or IDLE_CLOSE seconds standing still. Time is
-- simulation time: pauses and slow motion don't stretch the log.
--
-- Channel names follow those common in sim-racing MoTeC files. Signs: lateral
-- acceleration positive to the left, longitudinal positive when speeding up,
-- vertical with gravity removed, yaw rate and steering positive to the right.
-- The game's acceleration sensor shakes with the chassis (a couple of m/s^2
-- from step to step), so accelerations and yaw rate are smoothed (SMOOTHING).

local M = {}

local ffi = require("ffi")
local bit = require("bit")
local band, rshift = bit.band, bit.rshift
local abs, floor, max, min, fmt, concat = math.abs, math.floor, math.max, math.min, string.format, table.concat

local DIR = "beamlog/"
local SETTINGS_FILE = DIR .. "settings.json"
local RATES = { [20] = true, [50] = true, [100] = true, [200] = true }
local IDLE_CLOSE = 60     -- s standing still before a run is closed
local UI_EVERY = 0.25     -- s between status updates to the UI app
local MOVING = 0.3        -- m/s
local SMOOTHING = 0.1     -- s, time constant for accelerations and yaw rate
local G = 9.80665
local DEG = 180 / math.pi
local METRES_PER_DEGREE = 111320
local FIRST_CAPACITY = 4096

local settings = { auto = true, rate = 50 }

local run = nil           -- the run being recorded
local saving = {}         -- finished runs being written, oldest first
local lastFile = nil
local held = false        -- stopped by hand: don't start again on movement
local sinceUi = 0
local smooth = { x = 0, y = 0, z = -G, yaw = 0 }

local function ev(name) return electrics.values[name] or 0 end

---------------------------------------------------------------- settings

local function loadSettings()
  local ok, s = pcall(jsonReadFile, SETTINGS_FILE)
  if ok and type(s) == "table" then
    if type(s.auto) == "boolean" then settings.auto = s.auto end
    if RATES[tonumber(s.rate) or -1] then settings.rate = tonumber(s.rate) end
  end
end

local function saveSettings()
  pcall(function() FS:directoryCreate(DIR, true) end)
  pcall(jsonWriteFile, SETTINGS_FILE, settings, true)
end

---------------------------------------------------------------- channels

local function findEngine()
  if not powertrain or not powertrain.getDevicesByType then return nil end
  local engines = powertrain.getDevicesByType("combustionEngine")
  return engines and engines[1] or nil
end

local function wheelsByName()
  local list, seen = {}, {}
  for _, wd in pairs(wheels.wheelRotators or {}) do
    if wd.name and not seen[wd.name] then
      seen[wd.name] = true
      list[#list + 1] = wd
    end
  end
  table.sort(list, function(a, b) return a.name < b.name end)
  return list
end

local function speed() return obj:getVelocity():length() end

-- { name, unit, decimal places, value() } in MoTeC units; per wheel at the end
local function buildChannels(engine)
  local function pos() return obj:getPosition() end
  local ch = {
    { "Ground Speed", "km/h", 2, function() return speed() * 3.6 end },
    { "Drive Wheel Speed", "km/h", 2, function() return ev("wheelspeed") * 3.6 end },
    { "Engine RPM", "rpm", 0, function()
      if engine and engine.outputAV1 then return abs(engine.outputAV1) * 30 / math.pi end
      return ev("rpm")
    end },
    { "Gear", "", 0, function() return ev("gearIndex") end },
    { "Throttle Pos", "%", 1, function() return ev("throttle") * 100 end },
    { "Brake Pos", "%", 1, function() return ev("brake") * 100 end },
    { "Clutch Pos", "%", 1, function() return ev("clutch") * 100 end },
    { "Handbrake Pos", "%", 1, function() return ev("parkingbrake") * 100 end },
    { "Steering Angle", "deg", 1, function() return -ev("steering") end },
    { "Steering Input", "%", 1, function() return -ev("steering_input") * 100 end },
    { "Engine Torque", "Nm", 1, function() return engine and engine.outputTorque1 or 0 end },
    { "Engine Load", "%", 1, function() return (engine and engine.instantEngineLoad or 0) * 100 end },
    { "Boost Pressure", "kPa", 1, function() return ev("turboBoost") * 6.894757 end },
    { "Water Temp", "C", 1, function() return ev("watertemp") end },
    { "Oil Temp", "C", 1, function() return ev("oiltemp") end },
    { "Fuel Level", "%", 1, function() return ev("fuel") * 100 end },
    { "ABS Active", "", 0, function() return ev("absActive") end },
    { "TC Active", "", 0, function() return ev("tcsActive") end },
    { "CG Accel Lateral", "G", 3, function() return smooth.x / G end },
    { "CG Accel Longitudinal", "G", 3, function() return -smooth.y / G end },
    { "CG Accel Vertical", "G", 3, function() return -smooth.z / G - 1 end },
    { "Chassis Yaw Rate", "deg/s", 2, function() return smooth.yaw * DEG end },
    { "Car Coord X", "m", 2, function() return pos().x end },
    { "Car Coord Y", "m", 2, function() return pos().y end },
    { "Car Coord Z", "m", 2, function() return pos().z end },
    { "GPS Latitude", "deg", 7, function() return pos().y / METRES_PER_DEGREE end },
    { "GPS Longitude", "deg", 7, function() return pos().x / METRES_PER_DEGREE end },
  }
  for _, wd in ipairs(wheelsByName()) do
    local n = wd.name
    local function ws() return abs(wd.angularVelocity or 0) * (wd.radius or 0) end
    ch[#ch + 1] = { "Wheel Speed " .. n, "km/h", 2, function() return ws() * 3.6 end }
    ch[#ch + 1] = { "Wheel Slip " .. n, "%", 1, function() return (ws() - speed()) / max(speed(), 1) * 100 end }
    ch[#ch + 1] = { "Wheel Angular Speed " .. n, "rad/s", 2, function() return abs(wd.angularVelocity or 0) end }
    ch[#ch + 1] = { "Tire Radius " .. n, "mm", 1, function() return (wd.radius or 0) * 1000 end }
    ch[#ch + 1] = { "Brake Temp " .. n, "C", 1, function()
      local th = electrics.values.wheelThermals
      return th and th[n] and th[n].brakeSurfaceTemperature or 0
    end }
  end
  return ch
end

-- "Ground Speed" -> "GSpeed": initials, then the last word, up to 8 characters
local function shortName(name)
  local words = {}
  for w in name:gmatch("%S+") do words[#words + 1] = w end
  local s = ""
  for i = 1, #words - 1 do s = s .. words[i]:sub(1, 1) end
  return (s .. (words[#words] or "")):sub(1, 8)
end

---------------------------------------------------------------- MoTeC files

local HEADER_SIZE, EVENT_SIZE, CHANNEL_SIZE = 1762, 1154, 116

local function u16(v) v = floor(v) % 65536; return string.char(band(v, 255), rshift(v, 8)) end
local function u32(v)
  v = floor(v) % 4294967296
  return string.char(band(v, 255), band(rshift(v, 8), 255), band(rshift(v, 16), 255), band(rshift(v, 24), 255))
end
local function zeros(n) return string.rep("\0", n) end
local function text(s, width)
  s = tostring(s or ""):gsub("[^\32-\126]", "?"):sub(1, width)
  return s .. zeros(width - #s)
end

-- everything before the channel data: header, event, channel headers
local function ldHead(r)
  local n, count = #r.channels, r.count
  local metaAt = HEADER_SIZE + EVENT_SIZE
  local dataAt = metaAt + n * CHANNEL_SIZE
  local p = {
    u32(0x40), zeros(4), u32(n > 0 and metaAt or 0), u32(dataAt), zeros(20), u32(HEADER_SIZE), zeros(24),
    u16(0), u16(0x4240), u16(0x000F), u32(12007), text("ADL", 8), u16(420), u16(0x80), u32(n), u16(10), u16(10),
    text(r.date, 16), zeros(16), text(r.time, 16), zeros(16),
    text("", 64), text(r.vehicle, 64), zeros(64), text("BeamNG.drive", 64), zeros(64), zeros(1024),
    u32(0x00D20822), zeros(66), text(r.config, 64), zeros(126),
    -- event
    text("BeamNG.drive", 64), text(r.session, 64), text("Recorded with Beamlog", 1024), u16(0),
  }
  for i, c in ipairs(r.channels) do
    local here = metaAt + (i - 1) * CHANNEL_SIZE
    p[#p + 1] = concat({
      u32(i == 1 and 0 or here - CHANNEL_SIZE), u32(i < n and here + CHANNEL_SIZE or 0),
      u32(dataAt + (i - 1) * count * 4), u32(count),
      u16(50000 + i - 1), u16(5), u16(4), u16(r.rate),
      u16(0), u16(1), u16(1), u16(c[3]),
      text(c[1], 32), text(shortName(c[1]), 8), text(c[2], 12), zeros(32),
    })
  end
  return concat(p)
end

local function lapTime(s)
  local ms = floor(s * 1000 + 0.5)
  return fmt("%d:%02d.%03d", floor(ms / 60000), floor(ms / 1000) % 60, ms % 1000)
end

local function ldx(laps, duration)
  local markers = {}
  for i, t in ipairs(laps) do
    markers[#markers + 1] = fmt('\t\t\t\t\t<Marker Version="100" ClassName="BCN" Name="Manual.%d" Flags="77" Time="%.6f"/>\n', i, t * 1e6)
  end
  local edges = { 0 }
  for _, t in ipairs(laps) do edges[#edges + 1] = t end
  edges[#edges + 1] = duration
  local details = { fmt('\t\t\t<String Id="Total Laps" Value="%d"/>\n', #edges - 1) }
  if #laps >= 2 then
    local best, bestI = math.huge, 0
    for i = 2, #edges - 2 do -- full laps lie between two marks
      local lt = edges[i + 1] - edges[i]
      if lt < best then best, bestI = lt, i - 1 end
    end
    details[#details + 1] = fmt('\t\t\t<String Id="Fastest Time" Value="%s"/>\n', lapTime(best))
    details[#details + 1] = fmt('\t\t\t<String Id="Fastest Lap" Value="%d"/>\n', bestI)
  end
  return '<?xml version="1.0" encoding="utf-8"?>\n<LDXFile Version="1.5">\n\t<Layers>\n\t\t<Layer>\n\t\t\t<MarkerBlock>\n'
    .. '\t\t\t\t<MarkerGroup Name="Beacons" Index="3">\n' .. concat(markers) .. '\t\t\t\t</MarkerGroup>\n\t\t\t</MarkerBlock>\n'
    .. '\t\t</Layer>\n\t\t<Details>\n' .. concat(details) .. '\t\t</Details>\n\t</Layers>\n</LDXFile>\n'
end

-- write one step of a finished run; true when the run is fully written
local function saveStep(job)
  if job.step == 0 then
    job.file = io.open(job.path .. ".ld", "wb")
    if not job.file then
      -- the folder couldn't be made: save in the user folder itself instead
      job.path = "beamlog_" .. job.run.session
      job.file = io.open(job.path .. ".ld", "wb")
    end
    if not job.file then
      if log then log("E", "beamlog", "couldn't write " .. job.path .. ".ld") end
      return true
    end
    job.file:write(ldHead(job.run))
  elseif job.step <= #job.run.channels then
    job.file:write(ffi.string(job.run.buffers[job.step], job.run.count * 4))
    job.run.buffers[job.step] = nil
  else
    job.file:close()
    local x = io.open(job.path .. ".ldx", "w")
    if x then
      x:write(ldx(job.run.laps, job.run.count / job.run.rate))
      x:close()
    end
    lastFile = job.path .. ".ld"
    return true
  end
  job.step = job.step + 1
  return false
end

local function finishAll()
  for _, job in ipairs(saving) do
    while not saveStep(job) do end
  end
  saving = {}
end

---------------------------------------------------------------- runs

local function safeName(s) return (tostring(s or "vehicle"):gsub("[^%w%-]+", "_")) end

local function configName()
  local pc = v and v.config and v.config.partConfigFilename
  if type(pc) == "string" then return pc:match("([^/]+)%.pc$") or pc end
  return ""
end

-- a path not used yet: base, base_2, ... if runs start within the same second
local function freePath(base)
  for n = 1, 99 do
    local path = base .. (n > 1 and ("_" .. n) or "")
    local f = io.open(path .. ".ld", "r")
    if f then f:close() else return path end
  end
  return base .. "_" .. os.time()
end

local function sendState()
  guihooks.trigger("BeamlogState", {
    recording = run ~= nil,
    held = held,
    auto = settings.auto,
    rate = settings.rate,
    time = run and run.count / run.rate or 0,
    laps = run and #run.laps or 0,
    saving = #saving,
    file = run and run.session or nil,
    lastFile = lastFile,
  })
end

local function openRun()
  pcall(function() FS:directoryCreate(DIR, true) end)
  local model = v and v.config and v.config.mainPartName or "vehicle"
  local config = configName()
  local base = os.date("%Y-%m-%d_%H-%M-%S") .. "_" .. safeName(model)
  local path = freePath(DIR .. base)
  local channels = buildChannels(findEngine())
  local buffers = {}
  for i = 1, #channels do buffers[i] = ffi.new("int32_t[?]", FIRST_CAPACITY) end
  -- the game calls onPhysicsStep for extensions only once this has been called
  if enablePhysicsStepHook then pcall(enablePhysicsStepHook) end
  run = {
    path = path, session = path:match("[^/]+$"), channels = channels, buffers = buffers,
    capacity = FIRST_CAPACITY, count = 0, rate = settings.rate, laps = {}, accum = 0, idle = 0,
    date = os.date("%d/%m/%Y"), time = os.date("%H:%M:%S"),
    vehicle = config ~= "" and (model .. " " .. config) or model, config = config,
  }
  sendState()
end

local function closeRun(now)
  if run then
    if run.count >= 2 then
      saving[#saving + 1] = { run = run, path = run.path, step = 0 }
    end
    run = nil
  end
  -- the vehicle is going away: finish every save still in progress, not only this run's
  if now then finishAll() end
  sendState()
end

local function grow(r)
  local cap = r.capacity * 2
  for i = 1, #r.buffers do
    local b = ffi.new("int32_t[?]", cap)
    ffi.copy(b, r.buffers[i], r.count * 4)
    r.buffers[i] = b
  end
  r.capacity = cap
end

local function sample(r)
  if r.count >= r.capacity then grow(r) end
  local k = r.count
  for i, c in ipairs(r.channels) do
    local ok, val = pcall(c[4])
    val = ok and tonumber(val) or 0
    if val ~= val then val = 0 end
    val = floor(val * 10 ^ c[3] + 0.5)
    r.buffers[i][k] = min(max(val, -2147483648), 2147483647)
  end
  r.count = k + 1
end

---------------------------------------------------------------- hooks

local function onPhysicsStep(dt)
  if not run then return end
  run.physics = true
  local a = min(dt / SMOOTHING, 1)
  smooth.x = smooth.x + ((sensors.gx or 0) - smooth.x) * a
  smooth.y = smooth.y + ((sensors.gy or 0) - smooth.y) * a
  smooth.z = smooth.z + ((sensors.gz or -G) - smooth.z) * a
  smooth.yaw = smooth.yaw + (obj:getYawAngularVelocity() - smooth.yaw) * a
  run.accum = run.accum + dt
  local period = 1 / run.rate
  if run.accum >= period then
    run.accum = run.accum - period
    sample(run)
  end
end

local function updateGFX(dt)
  if #saving > 0 and saveStep(saving[1]) then
    table.remove(saving, 1)
    sendState()
  end
  if not playerInfo.firstPlayerSeated then
    if run then closeRun() end
    return
  end
  if dt <= 0 then return end -- paused

  local moving = speed() > MOVING or ev("throttle") > 0.05
  if not run and settings.auto and not held and moving then openRun() end
  if run and not run.physics then
    -- no physics steps arriving: sample here, at the same fixed rate
    run.accum = run.accum + dt
    local period = 1 / run.rate
    while run.accum >= period do
      run.accum = run.accum - period
      sample(run)
    end
  end
  if run then
    run.idle = moving and 0 or run.idle + dt
    if run.idle >= IDLE_CLOSE then closeRun() end
  end

  sinceUi = sinceUi + dt
  if sinceUi >= UI_EVERY then
    sinceUi = 0
    sendState()
  end
end

---------------------------------------------------------------- controls

local function start()
  held = false
  if not run and playerInfo.firstPlayerSeated then openRun() end
  sendState()
end

local function stop()
  held = true
  closeRun()
end

local function split()
  if run then
    closeRun()
    openRun()
  else
    start()
  end
end

local function lap()
  if run then
    run.laps[#run.laps + 1] = run.count / run.rate
    sendState()
  end
end

local function setRate(hz)
  hz = tonumber(hz)
  if not RATES[hz or -1] then return end
  settings.rate = hz
  saveSettings()
  -- a run keeps one rate: carry on in a new file at the new one
  if run then split() else sendState() end
end

local function setAuto(on)
  settings.auto = on and true or false
  saveSettings()
  sendState()
end

loadSettings()

M.onPhysicsStep = onPhysicsStep
M.updateGFX = updateGFX
M.onReset = function() closeRun() end
M.onVehicleDestroyed = function() closeRun(true) end
M.onExtensionUnloaded = function() closeRun(true) end
M.start = start
M.stop = stop
M.split = split
M.lap = lap
M.setRate = setRate
M.setAuto = setAuto
M.requestState = sendState

return M
