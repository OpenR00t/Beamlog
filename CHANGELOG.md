# Changelog

## Unreleased

- Fixed: switching or respawning the vehicle while a stopped run was still being saved left the `.ld` with only some of its channels and no `.ldx`. The save is now finished before the vehicle goes.

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
