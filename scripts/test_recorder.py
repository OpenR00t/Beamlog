#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 The Beamlog contributors
"""Run the in-game recorder outside the game and check the MoTeC files it writes.

The recorder runs under LuaJIT (the `lupa` package) with small stand-ins for
the game's objects: a car that pulls away, turns right, and stops. The .ld is
then read back with an independent parser written here from the format notes.

    pip install lupa
    python scripts/test_recorder.py
"""
import math
import os
import pathlib
import re
import struct
import sys
import tempfile

from lupa import luajit21

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "mod" / "beamlog" / "lua" / "vehicle" / "extensions" / "auto" / "beamlog.lua"

GAME = r"""
vec = function(x, y, z) return { x = x, y = y, z = z, length = function(s) return math.sqrt(s.x^2 + s.y^2 + s.z^2) end } end
car = { v = 0, x = 0, y = 0, h = 0, yaw = 0, ax = 0, ay = 0 }
obj = {
  getVelocity = function() return vec(car.v, 0, 0) end,
  getPosition = function() return vec(car.x, car.y, 3) end,
  getYawAngularVelocity = function() return car.yaw end,
}
electrics = { values = { gearIndex = 1, throttle = 0, steering = 0, steering_input = 0, wheelspeed = 0,
  watertemp = 90, oiltemp = 95, fuel = 0.5, wheelThermals = { RL = { brakeSurfaceTemperature = 120 } } } }
sensors = { gx = 0, gy = 0, gz = -9.81 }
rl = { name = "RL", angularVelocity = 0, radius = 0.3 }
wheels = { wheelRotators = { [0] = rl, { name = "RR", angularVelocity = 0, radius = 0.3 } } }
engine = { outputAV1 = 100, outputTorque1 = 300, instantEngineLoad = 1 }
powertrain = { getDevicesByType = function() return { engine } end }
playerInfo = { firstPlayerSeated = true }
v = { config = { mainPartName = "testcar", partConfigFilename = "vehicles/testcar/sport.pc" } }
guihooks = { trigger = function(name, s) lastState = s end }
saved = nil
jsonReadFile = function() return saved end
jsonWriteFile = function(p, t) saved = { auto = t.auto, rate = t.rate } end
FS = { directoryCreate = function(self, d) end }  -- the test makes the folder itself
hookEnabled = false
enablePhysicsStepHook = function() hookEnabled = true end

-- drive: 10 s pulling away at 3 m/s^2, then 10 s turning right; 2000 physics steps a second
function drive(M, seconds, accel, yaw)
  local steps = math.floor(seconds * 2000)
  for i = 1, steps do
    local dt = 1 / 2000
    car.v = car.v + accel * dt
    car.yaw = yaw                      -- the game: positive to the right
    car.h = car.h + yaw * dt
    car.x = car.x + car.v * math.sin(car.h) * dt
    car.y = car.y + car.v * math.cos(car.h) * dt
    local shake = (i % 2 == 0) and 2 or -2
    sensors.gx = -yaw * car.v + shake  -- x to the left: a right turn pulls to the right
    sensors.gy = -accel - shake        -- y to the rear
    electrics.values.steering = -yaw * 100  -- the game steers positive to the left
    electrics.values.throttle = accel > 0 and 1 or 0
    rl.angularVelocity = car.v / 0.3 * 1.05
    engine.outputAV1 = 100 + car.v * 20
    M.onPhysicsStep(dt)
    if i % 33 == 0 then M.updateGFX(1 / 60) end
  end
end
"""


def read_ld(path):
    b = pathlib.Path(path).read_bytes()
    assert struct.unpack_from("<I", b, 0)[0] == 0x40, "marker"
    assert b[94:104] != b"\0" * 10, "date"
    assert struct.unpack_from("<I", b, 1502)[0] == 0x00D20822, "pro logging"
    out, p = {}, struct.unpack_from("<I", b, 8)[0]
    while p:
        prev, nxt, data, n, _, kind, size, rate, shift, mul, scale, dec = struct.unpack_from("<IIIIHHHHhhhh", b, p)
        name = b[p + 32:p + 64].rstrip(b"\0").decode()
        unit = b[p + 72:p + 84].rstrip(b"\0").decode()
        assert (kind, size) == (5, 4), name
        vals = [(r / scale * 10 ** -dec + shift) * mul for r in struct.unpack_from("<%di" % n, b, data)]
        out[name] = (unit, rate, vals)
        p = nxt
    return out


def main():
    work = os.environ.get("BEAMLOG_TEST_DIR") or tempfile.mkdtemp()
    os.makedirs(work, exist_ok=True)
    os.chdir(work)
    os.makedirs("beamlog", exist_ok=True)
    lua = luajit21.LuaRuntime()
    lua.execute(GAME)
    M = lua.execute(SCRIPT.read_text())
    drive = lua.globals().drive

    M.updateGFX(1 / 60)
    M.requestState()
    assert not lua.eval("lastState.recording"), "should wait for the car to move"
    drive(M, 10, 3.0, 0.0)
    assert lua.eval("hookEnabled"), "the physics-step hook must be switched on"
    M.lap()
    drive(M, 10, 0.0, 0.3)
    M.lap()
    drive(M, 2, 0.0, 0.3)
    M.stop()
    for _ in range(200):  # saving is spread over frames
        M.updateGFX(1 / 60)
    files = sorted(pathlib.Path(work, "beamlog").glob("*"))
    names = [f.name for f in files]
    assert any(n.endswith(".ld") for n in names) and any(n.endswith(".ldx") for n in names), names
    ld = next(f for f in files if f.suffix == ".ld")
    ch = read_ld(ld)

    unit, rate, speed = ch["Ground Speed"]
    assert (unit, rate) == ("km/h", 50), (unit, rate)
    assert abs(len(speed) - 22 * 50) <= 2, len(speed)
    assert abs(max(speed) - 108) < 0.5, max(speed)
    at = lambda name, s: ch[name][2][int(s * 50)]
    assert 0.28 < at("CG Accel Longitudinal", 5) < 0.33, at("CG Accel Longitudinal", 5)
    assert at("CG Accel Lateral", 15) < -0.8, at("CG Accel Lateral", 15)  # right turn: acceleration to the right
    assert abs(at("CG Accel Vertical", 5)) < 0.02, at("CG Accel Vertical", 5)
    assert at("Chassis Yaw Rate", 15) > 17, at("Chassis Yaw Rate", 15)
    assert at("Steering Angle", 15) > 0, at("Steering Angle", 15)
    assert 4 < at("Wheel Slip RL", 15) < 6, at("Wheel Slip RL", 15)
    assert at("Brake Temp RL", 15) == 120
    assert ch["GPS Latitude"][0] == "deg"

    ldx = ld.with_suffix(".ldx").read_text()
    times = [float(t) / 1e6 for t in re.findall(r'Time="([0-9.]+)"', ldx)]
    assert len(times) == 2 and abs(times[0] - 10) < 0.05 and abs(times[1] - 20) < 0.05, times
    assert 'Id="Fastest Time" Value="0:10.000"' in ldx, ldx

    # without physics steps (hook never delivered), frames still record a run
    lua.execute("electrics.values.throttle = 1; car.v = 10")
    M.start()
    for _ in range(120):
        M.updateGFX(1 / 60)
    M.stop()
    for _ in range(200):
        M.updateGFX(1 / 60)
    lds = sorted(pathlib.Path(work, "beamlog").glob("*.ld"))
    assert len(lds) == 2, lds
    _, rate, sp = read_ld(lds[-1])["Ground Speed"]
    assert rate == 50 and 95 <= len(sp) <= 101, (rate, len(sp))

    # settings persist; changing the rate during a run starts a new file
    M.setRate(100)
    assert lua.eval("saved.rate") == 100
    # no beamlog folder (it couldn't be made): the run is saved in the user folder itself
    bare = tempfile.mkdtemp()
    os.chdir(bare)
    lua2 = luajit21.LuaRuntime()
    lua2.execute(GAME)
    M2 = lua2.execute(SCRIPT.read_text())
    M2.start()
    lua2.globals().drive(M2, 2, 3.0, 0.0)
    M2.stop()
    for _ in range(200):
        M2.updateGFX(1 / 60)
    fallback = sorted(p.name for p in pathlib.Path(bare).glob("beamlog_*"))
    assert any(n.endswith(".ld") for n in fallback) and any(n.endswith(".ldx") for n in fallback), fallback
    os.chdir(work)

    print(f"ok: {len(ch)} channels, {len(speed)} samples, files {names}")


if __name__ == "__main__":
    sys.exit(main())
