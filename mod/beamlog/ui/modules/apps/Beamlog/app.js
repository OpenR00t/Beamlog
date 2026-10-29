// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Beamlog contributors
//
// Beamlog UI app: shows whether the recorder is running and controls it.
// Talks to the vehicle extension (lua/vehicle/extensions/auto/beamlog.lua)
// on the vehicle being driven; the extension reports back with BeamlogState.
// Runs are saved as MoTeC .ld/.ldx files in the user folder's beamlog/.
angular.module('beamng.apps')
.directive('beamlogRecorder', [function () {
  return {
    template: `
<div class="bl">
  <style>
    .bl { width:100%; height:100%; box-sizing:border-box; padding:8px 10px; background:rgba(29,32,33,.88);
          color:#ebdbb2; font-family:'Roboto Mono', Consolas, monospace; font-size:12px; border-radius:4px;
          display:flex; flex-direction:column; gap:6px; overflow:hidden; }
    .bl .top { display:flex; align-items:center; gap:8px; }
    .bl .dot { width:10px; height:10px; border-radius:50%; background:#665c54; flex:none; }
    .bl .dot.rec { background:#fb4934; box-shadow:0 0 6px #fb4934; }
    .bl .dot.wait { background:#fabd2f; }
    .bl .title { font-weight:700; letter-spacing:.08em; }
    .bl .status { color:#a89984; margin-left:auto; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; min-width:0; font-size:11px; }
    .bl .row { display:flex; gap:6px; align-items:center; }
    .bl button { flex:1; background:#3c3836; color:#ebdbb2; border:1px solid #504945; border-radius:3px;
                 padding:4px 0; font:inherit; cursor:pointer; }
    .bl button:hover { background:#504945; }
    .bl button.on { border-color:#fb4934; color:#fb4934; }
    .bl button:disabled { opacity:.4; cursor:default; }
    .bl select { background:#3c3836; color:#ebdbb2; border:1px solid #504945; border-radius:3px; font:inherit; }
    .bl label { display:flex; align-items:center; gap:4px; color:#a89984; cursor:pointer; }
    .bl .file { color:#7c6f64; font-size:10px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  </style>
  <div class="top">
    <span class="dot" ng-class="{rec: s.recording, wait: !s.recording && s.auto && !s.held}"></span>
    <span class="title">BEAMLOG</span>
    <span class="status">{{ statusText() }}</span>
  </div>
  <div class="row">
    <button ng-class="{on: s.recording}" ng-click="toggle()">{{ s.recording ? 'Stop' : 'Start' }}</button>
    <button ng-click="call('split()')" ng-disabled="!s.recording" title="End this run and start the next file">Split</button>
    <button ng-click="call('lap()')" ng-disabled="!s.recording" title="Mark a lap here">Lap</button>
  </div>
  <div class="row">
    <span style="color:#a89984">Rate</span>
    <select ng-model="rate" ng-change="call('setRate(' + rate + ')')"
            ng-options="r.v as r.t for r in rates"></select>
    <label title="Start a run when the car moves"><input type="checkbox" ng-model="auto"
           ng-change="call('setAuto(' + (auto ? 'true' : 'false') + ')')"> auto</label>
  </div>
  <div class="file" title="{{ s.file || s.lastFile }}">{{ s.file ? s.file : (s.lastFile ? 'saved ' + s.lastFile : 'no run yet') }}</div>
</div>`,
    replace: true,
    restrict: 'EA',
    scope: true,
    link: function (scope) {
      scope.s = { recording: false, held: false, auto: true, rate: 50, time: 0, laps: 0, saving: 0 }
      scope.rates = [{ v: 20, t: '20 Hz' }, { v: 50, t: '50 Hz' }, { v: 100, t: '100 Hz' }, { v: 200, t: '200 Hz' }]
      scope.rate = 50
      scope.auto = true

      scope.call = function (fn) {
        bngApi.activeObjectLua('if extensions.beamlog then extensions.beamlog.' + fn + ' end')
      }
      scope.toggle = function () {
        scope.call(scope.s.recording ? 'stop()' : 'start()')
      }
      function clock(sec) {
        sec = Math.floor(sec || 0)
        var m = Math.floor(sec / 60), s = sec % 60
        return m + ':' + (s < 10 ? '0' : '') + s
      }
      scope.statusText = function () {
        var s = scope.s
        if (s.recording) return 'REC ' + clock(s.time) + (s.laps ? ' · lap ' + (s.laps + 1) : '')
        if (s.saving) return 'saving…'
        if (s.held || !s.auto) return 'stopped'
        return 'armed'
      }

      scope.$on('BeamlogState', function (event, data) {
        scope.$evalAsync(function () {
          scope.s = data
          scope.rate = data.rate
          scope.auto = data.auto
        })
      })
      scope.$on('VehicleFocusChanged', function () { scope.call('requestState()') })
      scope.call('requestState()')
    }
  }
}])
