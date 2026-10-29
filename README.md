# Beamlog

Records what a car does in [BeamNG.drive](https://www.beamng.com/) and saves it as
[MoTeC i2](https://www.motec.com.au/i2/i2overview/) log files, so a drive can be studied
the way a race team studies a session: speed, revs, gears, inputs, g-forces, wheel speeds
and slip, brake temperatures and a track map, against time or distance.

## Quick start

1. From the [latest release](../../releases/latest), download **beamlog-mod.zip** and put
   it, still zipped, in the `mods` folder of your BeamNG.drive user folder. Restart the
   game.
2. Add the **Beamlog** app to your UI layout (UI apps, Debug) and drive. With *auto* on, a
   run starts when the car moves; when it ends it's saved as MoTeC files in the `beamlog`
   folder of your user folder.
3. Download **beamlog-workspace.zip** and unzip it into `Documents\MoTeC\i2\Workspaces`
   (you get a `Beamlog` folder there). In MoTeC i2 Pro, pick it with File > Workspaces.
4. Open your runs in i2. The user folder is under `AppData`, which Windows hides, so
   i2's file browser won't show it. Make a link to the log folder in Documents once, from
   Command Prompt:

   ```
   mklink /J "%USERPROFILE%\Documents\Beamlog" "%LOCALAPPDATA%\BeamNG\BeamNG.drive\current\beamlog"
   ```

   and open runs from `Documents\Beamlog` (the same files, updated as you record). Or
   paste the second path into the address bar of i2's open dialog.

## The recorder

The mod (`mod/beamlog`) is a script that runs on the car you're driving
(`lua/vehicle/extensions/auto/beamlog.lua`) and a UI app to control it
(`ui/modules/apps/Beamlog`).

- **Auto** (on by default): a run starts when the car first moves. Turn it off to record
  only when you press **Start**.
- **Stop** ends the run; **Split** ends it and starts the next one straight away; **Lap**
  marks a lap, which MoTeC shows as laps.
- **Rate**: 20, 50 (default), 100 or 200 samples a second, taken on the physics step, so
  even the high rates are real for speed, wheel speeds, acceleration and engine speed.
  Values the game updates once per frame (your inputs, temperatures) repeat between
  frames. Changing the rate during a run starts a new one.
- A run also ends when you reset or leave the vehicle, it's removed, or it has stood
  still for 60 seconds.
- Each run is saved as `beamlog/<date>_<time>_<vehicle>.ld` (and `.ldx`, which holds the
  laps) in the user folder. Saving is spread over a few frames, so it doesn't stall the
  game. Settings are kept in `beamlog/settings.json`.
- Time is simulation time: pausing or slow motion doesn't stretch the log.
- Only the car you're seated in is recorded, not traffic or other computer-driven cars.
- Without the app, the same controls work from the vehicle Lua console:
  `extensions.beamlog.start()`, `.stop()`, `.split()`, `.lap()`, `.setRate(100)`,
  `.setAuto(false)`.

From source, copy `mod/beamlog` into the user folder as `mods/unpacked/beamlog`.

## Channels

| Channel | Unit | From |
| --- | --- | --- |
| Ground Speed, Drive Wheel Speed | km/h | body velocity; the drivetrain's wheel speed |
| Engine RPM, Gear | rpm | the engine's own speed |
| Throttle Pos, Brake Pos, Clutch Pos, Handbrake Pos | % | the inputs the car receives |
| Steering Angle, Steering Input | deg, % | |
| Engine Torque, Engine Load | Nm, % | the combustion engine, if the car has one |
| Boost Pressure | kPa | turbocharger, if fitted |
| Water Temp, Oil Temp, Fuel Level | C, C, % | |
| ABS Active, TC Active | | |
| CG Accel Lateral, Longitudinal, Vertical | G | the vehicle's acceleration sensor |
| Chassis Yaw Rate | deg/s | |
| Body Roll, Body Pitch | deg | the body's attitude, from its forward and up directions |
| Body Roll Rate, Body Pitch Rate | deg/s | |
| Car Coord X, Y, Z | m | world position |
| GPS Latitude, Longitude | deg | world position as degrees near (0, 0), for i2's track map |
| Wheel Speed, Wheel Slip, Wheel Angular Speed, Tire Radius, Brake Temp (per wheel) | km/h, %, rad/s, mm, C | each wheel, named as the car names them (FL, FR, RL, RR) |

Names follow those common in sim-racing MoTeC files, so existing worksheets find them.
Signs: lateral acceleration positive to the left; longitudinal positive when speeding up;
vertical with gravity removed (0 standing still); yaw rate and steering positive to the
right; body roll positive with the right side down (as when cornering left), body pitch
positive nose up, and their rates the same way. The game's acceleration sensor shakes with the chassis (a couple of m/s² from one
step to the next), so accelerations and yaw, roll and pitch rates are smoothed with a 0.1 s time constant.

## The workspace

`scripts/build_workspace.py` writes a MoTeC i2 workspace laid out for these channels:

- **Driver**: a run overview (speed, revs, gear, pedals, steering, track map, g-g) and inputs.
- **Engine & Gearing**: a gearing map (revs against speed, coloured by gear), shifts,
  torque and power against revs coloured by throttle, temperatures and load, time at revs.
- **Chassis**: the g-g diagram and accelerations; steering against yaw rate; body roll and pitch, with roll against lateral g (the roll gradient).
- **Wheels & Brakes**: wheel speeds and slip, traction (rear slip against throttle),
  brake temperatures and ABS.

It adds calculated channels Engine Power, Overall Ratio, Front Slip and Rear Slip.

## The beamlog tool (optional)

A small command-line tool, `beamlog.exe` in the releases, or `cargo build --release` with
Rust 1.80 or later:

```text
beamlog info FILE ...       a summary of a run (.ld): top speed, gears, upshifts, laps, 0-100
beamlog check FILE.ld ...   the channels in a file, with their ranges
beamlog                     convert CSV logs from earlier versions of the recorder
beamlog watch               keep converting them as they appear
```

## The MoTeC format

The `.ld` format isn't published. The writers (in the recorder, and `src/ld.rs` in the
tool) follow the layout found by reverse engineering and used by other sim-racing
exporters: a header, an event record, a linked list of channel headers, then each
channel's samples as 32-bit integers scaled by its decimal places. The `.ldx` beside it is
XML that holds the lap marks.

## Releases

Pushing a tag such as `v0.1.0` runs `.github/workflows/release.yml`: it runs the tests,
builds `beamlog.exe`, and `scripts/package.py` makes `beamlog-mod.zip` and
`beamlog-workspace.zip`; all three are published on a GitHub release. `ci.yml` checks
formatting, lints and tests every push.

## Tests

- `cargo test`: the tool's MoTeC writer and reader, log parsing, resampling and signs.
- `python scripts/test_recorder.py` (needs `pip install lupa`): runs the in-game recorder
  under LuaJIT with stand-ins for the game, drives a car through a pull-away, a right-hand
  turn and two lap marks, and checks the `.ld` and `.ldx` it writes, read back by a
  separate parser.

## License

[GNU AGPL v3.0 or later](LICENSE). Copyright (C) 2026 The Beamlog contributors.
