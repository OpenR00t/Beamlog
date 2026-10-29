# Changelog

## Unreleased

- Added Body Roll, Body Pitch, Body Roll Rate and Body Pitch Rate channels, and a Body worksheet in the workspace (roll and pitch against time, and the roll gradient: body roll against lateral g).
- Fixed: swapping or removing the vehicle lost the run. A run still recording was never saved, and one still being written was left with only some of its channels and no `.ldx`. The game doesn't call the vehicle-side extension hooks for this, so the recorder now chains onto the global `onDespawnObject` and saves everything before the vehicle goes.

## 0.1.0

First release.

- Recorder mod for BeamNG.drive: records the car you're driving at 20 to 200 Hz on the
  physics step and saves each run as MoTeC i2 `.ld` and `.ldx` files, with no tool to
  run afterwards. Speed, engine, inputs, g-forces, yaw rate, position and a track map,
  and per-wheel speed, slip and brake temperature.
- Beamlog UI app: start and stop, split runs, mark laps, set the rate, auto-start on
  movement.
- MoTeC i2 workspace for Beamlog's channels: Driver, Engine & Gearing (gearing map,
  torque and power maps, shifts, temperatures), Chassis (g-g diagram, yaw) and
  Wheels & Brakes (speeds, slip, traction, brake temperatures).
- Optional `beamlog` tool: `info` and `check` on recorded runs.
