#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 The Beamlog contributors
"""Write the Beamlog workspace for MoTeC i2 Pro.

A workspace is a folder of XML files: Workspace.i2wsp lists the workbooks,
each workbook (.i2wkb) holds worksheets of graphs, scatter plots, histograms
and a track map that refer to channels by name, and Maths/User.xml adds
calculated channels. This one is laid out for the channels Beamlog records.

    python scripts/build_workspace.py [OUT_DIR]   (default: dist/workspace/Beamlog)

Copy the folder into Documents/MoTeC/i2/Workspaces and pick it in i2
(File > Workspaces).
"""
import pathlib
import sys
from xml.sax.saxutils import quoteattr

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCALE = 'Locale="English_United States.1252" DefaultLocale="C"'
WHEELS = ["FL", "FR", "RL", "RR"]


def a(**kw):
    """XML attributes from keyword arguments (trailing _ dropped, __ becomes '.')."""
    return " ".join(f"{k.rstrip('_').replace('__', '.')}={quoteattr(str(v))}" for k, v in kw.items())


# ---------------------------------------------------------------- components

def decimals(channel):
    if channel in ("Engine RPM", "Gear", "ABS Active", "TC Active"):
        return 0
    if channel.startswith("CG Accel"):
        return 2
    return 1


def time_distance(name, pos, groups):
    """Stacked graphs against time or distance; groups: [[channel, ...], ...]."""
    out = [f'<MoTeC.TimeDistance {a(Version=100, Name=name, PosPC=pos, ShowCaption=0, AutoCaption=0, LinkZoom=1, ModeId=0, ShowDataLegend=1, ShowColorLegend=1, TelemetryZoom=0, TraceLineWidth=1, TracePointSize=3, TraceStyle=0, LinkCursor=1, LinkDatum=1, ShowKeyValues=1, ShowKeyStats=0, KeyMode=0, KeyPos=1, ShowGridLines=1, ShowXAxis=1, ShowXAxisScrollBar=0, ShowYAxis=1, ShowYAxisScrollBar=1, ShowMarkers=1, ShowVSP=0, ShowDeviveEvents=1, ShowBands=1, BandGroup="Outings:Laps:Track Sections:Default", BandMode=1, OverlapMode=0, VariancePos=0.25, VarianceAutoScale=1)}>',
           f'<Graphs ActivateChild="Group 1">']
    edges = [round(i * 1000 / len(groups)) for i in range(len(groups) + 1)]
    for i, chans in enumerate(groups):
        out.append(f'<Graph {a(Name=f"Group {i + 1}", ScaleMode=2, ScaleMin=0, ScaleMax=1)}>')
        out.append(f'<Edges Mode="0" TopEdge="{edges[i]}" BottomEdge="{edges[i + 1]}"/>')
        out.append('<Edges Mode="1" TopEdge="0" BottomEdge="1000"/>')
        out.append('<Lines><Line Index="0" Enabled="1" Value="0" Color="128,128,128"/>'
                   '<Line Index="1" Enabled="0" Value="0" Color="0,0,248"/>'
                   '<Line Index="2" Enabled="0" Value="0" Color="248,0,0"/></Lines>')
        out.append(f'<Traces ActiveTrace={quoteattr(chans[0])}>')
        out += [f'<Trace {a(Id=c, DisplayUnit="", DisplayDPS=decimals(c), Visible=1)}/>' for c in chans]
        out.append("</Traces></Graph>")
    out.append('</Graphs><Cursor Version="1" Style="1" Snap="1"/></MoTeC.TimeDistance>')
    return "\n".join(out)


def gradient(c1, c2, f):
    return ",".join(str(round(x + (y - x) * f)) for x, y in zip(c1, c2))


def band_axis(lo, hi, count, c1=(64, 64, 255), c2=(255, 64, 64)):
    """Colour bands for a scatter's colour channel."""
    width = (hi - lo) / count
    col = lambda f: gradient(c1, c2, f)
    out = [f'<EvenWidthBandAxis {a(PaletteMode=1, GradientMode=1, BandChanColor="0,174,0", AxisChanColor="255,0,0", Color1=gradient(c1, c1, 0), Color2=gradient(c2, c2, 0), ReversGradient=0, CentredBands=0, GreaterMaxBand=1, LessMinBand=1, Continuous=0, BandMode=1, DataMin=lo, DataMax=hi, BandWidth=width, BandCount=count)}>',
           f'<ColorBand Flags="2" Min="{lo}" Max="{lo}" Color="{col(0)}" Visible="1"/>']
    for i in range(count):
        flags = 15 if i == count - 1 else 7
        out.append(f'<ColorBand Flags="{flags}" Min="{lo + i * width:g}" Max="{lo + (i + 1) * width:g}" Color="{col(i / max(count - 1, 1))}" Visible="1"/>')
    out.append(f'<ColorBand Flags="1" Min="{hi}" Max="{hi}" Color="{col(1)}" Visible="1"/>')
    out.append("</EvenWidthBandAxis>")
    return "\n".join(out)


def scatter(name, pos, x, ys, z, zrange, xr=None, yr=None):
    scale = {}
    if xr:
        scale.update(XScaleMode=1, XScaleMin=xr[0], XScaleMax=xr[1])
    else:
        scale.update(XScaleMode=0)
    if yr:
        scale.update(YScaleMode=1, YScaleMin=yr[0], YScaleMax=yr[1])
    else:
        scale.update(YScaleMode=0)
    head = a(Version=100, Name=name, PosPC=pos, ShowCaption=0, AutoCaption=0, LinkZoom=1, ModeId=0, ShowDataLegend=1, ShowColorLegend=1, TelemetryZoom=0, ActiveTrace=0, TileMode=1, TileActiveChan=0, TileAutoCols=1, TileCols=1, XChannel=x, ZChannel=z, **scale, ZScaleMode=0, ShowPoints=1, ShowDensity=0, DensityBins=50, DensityFillColorMode=1, DensityFillColor1="64,64,255", DensityFillColor2="255,64,64", IntervalStat=0, IntervalStat__BinWidth=100, ShowGridLines=1, ShowGradient=1, ShowColorBinSamples=1, Style=2, Size=1, ShowLines=0, ShowRegEquation=0, LinePointMode=1, CursorStyle=3, CursorSlope=0, CursorExtent=0, RegressionType=0, RegressionParam=0, RegressionZero=0, RegressionDigits=3)
    traces = "\n".join(f'<Trace {a(YChannel=y, Visible=1, YDelay=0)}/>' for y in ys)
    return f"<MoTeC.Scatter {head}>\n{traces}\n{band_axis(*zrange)}\n</MoTeC.Scatter>"


def track_map(name, pos):
    scheme = "".join(
        f'<Entry Name="{n}"><RenderMode Mode="SCREEN" RGB="{s}"/><RenderMode Mode="PRINT" RGB="{p}"/></Entry>'
        for n, s, p in [("CptDataBackground", "255,255,255", "255,255,255"), ("CptDataForeground", "0,0,0", "0,0,0"),
                        ("GPTRACK_DRIVEN_LINE_COLOR", "160,160,160", "0,0,0")])
    return (f'<MoTeC.GPS {a(Version=100, Name=name, Caption="Track", PosPC=pos, ShowCaption=0, AutoCaption=0, LinkZoom=1, ModeId="", ShowDataLegend=1, TelemetryZoom=0, XChannel="GPS Longitude", YChannel="GPS Latitude", ShowHeading=0, HChannel="GPS Heading", ShowWindSock=0, WChannel="Wind Direction", WInvert=0, ShowScale=1, Style=0, Size=0, ShowLines=1, TrackRender=6, Orientation=0, OrientationAngle=0, DataMode=1, ScaleMode=1, PanMode=0, CursorStyle=3, UseLocalColors=0, ShowBgMap=0)}>'
            f"<ComponentColorScheme>{scheme}</ComponentColorScheme></MoTeC.GPS>")


def histogram(name, pos, channel, lo, hi, width):
    count = round((hi - lo) / width)
    return (f'<MoTeC.Histogram {a(Version=100, Name=name, PosPC=pos, ShowCaption=0, AutoCaption=0, LinkZoom=1, ModeId=0, ShowDataLegend=1, ShowColorLegend=1, TelemetryZoom=0, ActiveTrace=0, TileMode=1, TileActiveChan=1, TileAutoCols=1, TileCols=1, SecondChannel="", XScaleMode=1, XSecondScaleMode=0, YUnitMode=1, YScaleMode=0, YScale=100, YAllowOverrides=0, YExcludeUntrusted=0, Style=1, BinLabelStep=-1, UseColors=0, InvertColors=0, StackLines=0, ShowColorBinSamples=1)}>'
            f'<HistoTrace {a(Id=channel, Visible=1)}/><HistoMainBins>{band_axis(lo, hi, count)}</HistoMainBins></MoTeC.Histogram>')


def worksheet(name, components):
    first = components[0].split('Name="', 1)[1].split('"', 1)[0]
    return (f'<Worksheet {a(Version=200, Name=name, LinkTo="Laps", DataDomain=1, LockByGPSTime=0)}>'
            f'<Controls Active={quoteattr(first)}>\n' + "\n".join(components) + "\n</Controls></Worksheet>")


def workbook(name, sheets):
    body = "\n".join(worksheet(n, c) for n, c in sheets)
    return (f'<?xml version="1.0"?>\n<Workbook {LOCALE} {a(Version=200, Name=name, Profile="Circuit", Active="Worksheets")}>\n'
            f'<Worksheets {a(Version=100, Name="Worksheets", PosPC="0,0,1000,1000", Active=sheets[0][0])}>\n{body}\n'
            "</Worksheets>\n</Workbook>\n")


# ---------------------------------------------------------------- the layout

def per_wheel(prefix):
    return [f"{prefix} {w}" for w in WHEELS]


WORKBOOKS = {
    "Driver": [
        ("Run", [
            time_distance("Run", "0,0,650,1000", [["Ground Speed"], ["Engine RPM"], ["Gear"], ["Throttle Pos", "Brake Pos"], ["Steering Angle"]]),
            track_map("Track", "650,0,1000,500"),
            scatter("G-G", "650,500,1000,1000", "CG Accel Lateral", ["CG Accel Longitudinal"], "Ground Speed", (0, 250, 10), (-2, 2), (-2, 2)),
        ]),
        ("Inputs", [
            time_distance("Inputs", "0,0,1000,1000", [["Throttle Pos"], ["Brake Pos"], ["Clutch Pos", "Handbrake Pos"], ["Steering Angle"], ["Gear"]]),
        ]),
    ],
    "Engine & Gearing": [
        ("Gearing Map", [
            scatter("Gearing Map", "0,0,1000,1000", "Ground Speed", ["Engine RPM"], "Gear", (0, 6, 6)),
        ]),
        ("Shifts", [
            time_distance("Shifts", "0,0,1000,1000", [["Engine RPM"], ["Gear"], ["Throttle Pos", "Clutch Pos"], ["Ground Speed"]]),
        ]),
        ("Torque", [
            scatter("Torque Map", "0,0,600,1000", "Engine RPM", ["Engine Torque"], "Throttle Pos", (0, 100, 10)),
            scatter("Power Map", "600,0,1000,1000", "Engine RPM", ["Engine Power"], "Throttle Pos", (0, 100, 10)),
        ]),
        ("Health", [
            time_distance("Health", "0,0,1000,1000", [["Water Temp", "Oil Temp"], ["Engine Load"], ["Boost Pressure"], ["Fuel Level"]]),
            ]),
        ("Rev Time", [
            histogram("RPM Histogram", "0,0,1000,1000", "Engine RPM", 0, 10000, 250),
        ]),
    ],
    "Chassis": [
        ("G-G", [
            scatter("G-G Diagram", "0,0,600,1000", "CG Accel Lateral", ["CG Accel Longitudinal"], "Ground Speed", (0, 250, 10), (-2, 2), (-2, 2)),
            time_distance("Accelerations", "600,0,1000,1000", [["CG Accel Lateral"], ["CG Accel Longitudinal"], ["CG Accel Vertical"]]),
        ]),
        ("Yaw", [
            time_distance("Yaw", "0,0,650,1000", [["Steering Angle"], ["Chassis Yaw Rate"], ["CG Accel Lateral"], ["Ground Speed"]]),
            scatter("Steering vs Yaw", "650,0,1000,1000", "Steering Angle", ["Chassis Yaw Rate"], "Ground Speed", (0, 250, 10)),
        ]),
        ("Body", [
            time_distance("Body", "0,0,650,1000", [["Body Roll"], ["Body Pitch"], ["Body Roll Rate", "Body Pitch Rate"], ["CG Accel Lateral", "CG Accel Longitudinal"]]),
            scatter("Roll Gradient", "650,0,1000,1000", "CG Accel Lateral", ["Body Roll"], "Ground Speed", (0, 250, 10), (-2, 2)),
        ]),
    ],
    "Wheels & Brakes": [
        ("Wheel Speeds", [
            time_distance("Wheel Speeds", "0,0,1000,1000", [["Ground Speed"] + per_wheel("Wheel Speed"), per_wheel("Wheel Slip"), ["Throttle Pos", "Brake Pos"]]),
        ]),
        ("Traction", [
            time_distance("Traction", "0,0,650,1000", [["Rear Slip", "Front Slip"], ["Throttle Pos"], ["TC Active"], ["Gear"]]),
            scatter("Slip vs Throttle", "650,0,1000,1000", "Throttle Pos", ["Rear Slip"], "Gear", (0, 6, 6)),
        ]),
        ("Brakes", [
            time_distance("Brakes", "0,0,1000,1000", [["Brake Pos"], per_wheel("Brake Temp"), ["ABS Active"], ["Ground Speed"]]),
        ]),
    ],
}

MATHS = [
    # id, script, unit, display decimals
    ("Engine Power", "'Engine Torque' [Nm] * 'Engine RPM' [rpm] * 0.000104719755", "kW", 1),
    ("Overall Ratio", "choose('Ground Speed' [km/h] > 5, 'Engine RPM' [rpm] / 'Ground Speed' [km/h], 0)", "", 2),
    ("Rear Slip", "('Wheel Slip RL' [%] + 'Wheel Slip RR' [%]) / 2", "%", 1),
    ("Front Slip", "('Wheel Slip FL' [%] + 'Wheel Slip FR' [%]) / 2", "%", 1),
]


def maths_xml():
    items = "\n".join(
        f'  <MathExpression {a(Id=i, Script=s, SampleRate=0, Unit=u, Active=1, Flags=0, Interpolate=1, DisplayColorIndex=n % 8, DisplayDPS=d, DisplayUnit=u)}/>'
        for n, (i, s, u, d) in enumerate(MATHS))
    return f'<?xml version="1.0" encoding="UTF-8"?>\n<Maths Id="User" Flags="191" {LOCALE}>\n <MathItems>\n{items}\n </MathItems>\n</Maths>\n'


def workspace_xml():
    xref = lambda n: quoteattr("Workbooks\\" + n + ".i2wkb")
    books = "\n".join(f"  <Workbook XRef={xref(n)} Name={quoteattr(n)}/>" for n in WORKBOOKS)
    first = next(iter(WORKBOOKS))
    docks = "\n".join(
        f'  <DockWindow Id="{d}" Visible="{v}" IsDocked="1" IsPinned="1" DockOffset="0" DockSide="35" DockIndex="{i}" Height="110" Width="220"/>'
        for i, (d, v) in enumerate([("Data", 1), ("Channels", 1), ("Reports", 1), ("Layouts", 1), ("Maths", 1)]))
    return f"""<?xml version="1.0"?>
<Analysis {LOCALE} Version="103" ShowOverlays="1" ExcludeUntrusted="1" IncludeOverlays="0" GraphTimeFormat="4" RemoveOnChanDblClick="1" ProfileId="Circuit" SetupSheetPasswdReqd="0" LapTimeDisplayFormat="0" LapTimeDisplayUnit="km/h" LapNameInOut="1" AutoLapNumber="0" LapTrustPC="0">
 <Resources/>
 <LatGTrackGen LateralGChannel="CG Accel Lateral"/>
 <DataSelect SelectFastestOnLoad="1" DefaultShowDisabled="0" MainColorSource="0">
  <DataSources/>
  <View Name="Laps" Color="128,240,0" View="Laps"/>
  <View Name="Outings" Color="240,128,0" View="Outings"/>
 </DataSelect>
 <ChannelsWindow/>
 <CAN/>
 <Workbooks Version="100" Name="My Workbooks" Active={quoteattr(first)}>
{books}
 </Workbooks>
 <DockWindows>
  <DockWindow Id="Alarms" Visible="0" IsDocked="1" IsPinned="1" DockOffset="0.25" DockSide="0" DockIndex="0" Height="160" Width="220"/>
{docks}
 </DockWindows>
 <TelemetrySettings/>
</Analysis>
"""


EMPTY = {
    "Alarms.xml": '<?xml version="1.0"?>\n',
    "Channels/Aliases.xml": f'<?xml version="1.0"?>\n<Aliases {LOCALE}/>\n',
    "Channels/Mappings.xml": f'<?xml version="1.0"?>\n<Items {LOCALE}/>\n',
    "Channels/Overrides.xml": f'<?xml version="1.0"?>\n<Overrides {LOCALE}/>\n',
    "PacejkaProfiles.xml": f'<?xml version="1.0"?>\n<PacejkaProfiles {LOCALE}/>\n',
    "Workbooks/Reports.xml": f'<?xml version="1.0"?>\n<Reports {LOCALE}/>\n',
}


def build(out: pathlib.Path):
    files = dict(EMPTY)
    files["Workspace.i2wsp"] = workspace_xml()
    files["Maths/User.xml"] = maths_xml()
    for name, sheets in WORKBOOKS.items():
        files[f"Workbooks/{name}.i2wkb"] = workbook(name, sheets)
    for rel, text in files.items():
        p = out / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="ascii", newline="\r\n")
    for d in ("CAN", "Setup Sheets", "Track Maps"):
        (out / d).mkdir(exist_ok=True)
    return files


def main():
    out = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "dist" / "workspace" / "Beamlog"
    files = build(out)
    print(f"{out} ({len(files)} files)")


if __name__ == "__main__":
    main()
