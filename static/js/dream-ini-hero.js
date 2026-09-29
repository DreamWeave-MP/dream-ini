// dream-ini's hero: the importer at work, drawn live with three.js from the program's own icon.
//
// The icon's circuit medallion is the importer: a lacquered board whose gold traces run in the
// icon's four quadrants around a diamond and a central ring, bound by a braided cord. On its left
// stands a page of Morrowind.ini, on its right the openmw.cfg being written. Lines leave the INI one
// by one as cards, dock at a pad on the left rim and run a trace into the ring, which is the parser.
// What comes out runs a trace on the right and flies to the cfg as the line dream-ini writes:
// GameFile0=Morrowind.esm becomes content=Morrowind.esm, Archive 0 brings Morrowind.bsa in first,
// [Weather Clear] and Cloud Texture=Tx_Sky_Clear.dds become Weather_Clear_Cloud_Texture, and the
// encoding= line is always set. [Fonts] keys, which need --fonts, and comments never reach the
// ring: they crumble, as the parser drops them.
//
// Select text in the hero's title or summary and the importer parses it as an INI line, by the
// crate's rules. Morrowind.ini imports the whole file: all 573 keys the crate knows pour through
// the board, the three [Fonts] keys fall away, and the cfg fills with 570 fallback= lines.
// openmw.cfg rewrites the cfg. Anything else has no = and is dropped, which is the only hint.
//
// Rendering: a half-float scene target with MSAA, a bright pass and two blur chains for bloom,
// ACES tone mapping and dithering, and alpha kept so the hero's own background shows through.
// Nothing runs while the hero is off screen or the tab is hidden, the resolution drops if frames
// run slow, and under prefers-reduced-motion one settled frame is drawn. Until the first frame,
// and without WebGL 2, a still of the scene stands in its place.

import * as THREE from './vendor/three.module.min.js';

// Every fallback key dream-ini imports, as src/fallback_keys.rs lists them: section, then keys.
const FALLBACK_SECTIONS = [
  ["LightAttenuation", "UseConstant|ConstantValue|UseLinear|LinearMethod|LinearValue|LinearRadiusMult|UseQuadratic|QuadraticMethod|QuadraticValue|QuadraticRadiusMult|OutQuadInLin"],
  ["Inventory", "DirectionalDiffuseR|DirectionalDiffuseG|DirectionalDiffuseB|DirectionalAmbientR|DirectionalAmbientG|DirectionalAmbientB|DirectionalRotationX|DirectionalRotationY|UniformScaling"],
  ["Map", "Travel Siltstrider Red|Travel Siltstrider Green|Travel Siltstrider Blue|Travel Boat Red|Travel Boat Green|Travel Boat Blue|Travel Magic Red|Travel Magic Green|Travel Magic Blue|Show Travel Lines"],
  ["Water", "Map Alpha|World Alpha|SurfaceTextureSize|SurfaceTileCount|SurfaceFPS|SurfaceTexture|SurfaceFrameCount|TileTextureDivisor|RippleTexture|RippleFrameCount|RippleLifetime|MaxNumberRipples|RippleScale|RippleRotSpeed|RippleAlphas|PSWaterReflectTerrain|PSWaterReflectUpdate|NearWaterRadius|NearWaterPoints|NearWaterUnderwaterFreq|NearWaterUnderwaterVolume|NearWaterIndoorTolerance|NearWaterOutdoorTolerance|NearWaterIndoorID|NearWaterOutdoorID|UnderwaterSunriseFog|UnderwaterDayFog|UnderwaterSunsetFog|UnderwaterNightFog|UnderwaterIndoorFog|UnderwaterColor|UnderwaterColorWeight"],
  ["PixelWater", "SurfaceFPS|TileCount|Resolution"],
  ["Fonts", "Font 0|Font 1|Font 2"],
  ["FontColor", "color_normal|color_normal_over|color_normal_pressed|color_active|color_active_over|color_active_pressed|color_disabled|color_disabled_over|color_disabled_pressed|color_link|color_link_over|color_link_pressed|color_journal_link|color_journal_link_over|color_journal_link_pressed|color_journal_topic|color_journal_topic_over|color_journal_topic_pressed|color_answer|color_answer_over|color_answer_pressed|color_header|color_notify|color_big_normal|color_big_normal_over|color_big_normal_pressed|color_big_link|color_big_link_over|color_big_link_pressed|color_big_answer|color_big_answer_over|color_big_answer_pressed|color_big_header|color_big_notify|color_background|color_focus|color_health|color_magic|color_fatigue|color_misc|color_weapon_fill|color_magic_fill|color_positive|color_negative|color_count"],
  ["Level Up", "Level2|Level3|Level4|Level5|Level6|Level7|Level8|Level9|Level10|Level11|Level12|Level13|Level14|Level15|Level16|Level17|Level18|Level19|Level20|Default"],
  ["Question 1", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 2", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 3", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 4", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 5", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 6", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 7", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 8", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 9", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Question 10", "Question|AnswerOne|AnswerTwo|AnswerThree|Sound"],
  ["Blood", "Model 0|Model 1|Model 2|Texture 0|Texture 1|Texture 2|Texture 3|Texture 4|Texture 5|Texture 6|Texture 7|Texture Name 0|Texture Name 1|Texture Name 2|Texture Name 3|Texture Name 4|Texture Name 5|Texture Name 6|Texture Name 7"],
  ["Movies", "Company Logo|Morrowind Logo|New Game|Loading|Options Menu"],
  ["Weather Thunderstorm", "Thunder Sound ID 0|Thunder Sound ID 1|Thunder Sound ID 2|Thunder Sound ID 3"],
  ["Weather", "Sunrise Time|Sunset Time|Sunrise Duration|Sunset Duration|Hours Between Weather Changes"],
  ["Weather Thunderstorm", "Thunder Frequency|Thunder Threshold"],
  ["Weather", "EnvReduceColor|LerpCloseColor|BumpFadeColor|AlphaReduce|Minimum Time Between Environmental Sounds|Maximum Time Between Environmental Sounds|Sun Glare Fader Max|Sun Glare Fader Angle Max|Sun Glare Fader Color|Timescale Clouds|Precip Gravity|Rain Ripples|Rain Ripple Radius|Rain Ripples Per Drop|Rain Ripple Scale|Rain Ripple Speed|Fog Depth Change Speed|Sky Pre-Sunrise Time|Sky Post-Sunrise Time|Sky Pre-Sunset Time|Sky Post-Sunset Time|Ambient Pre-Sunrise Time|Ambient Post-Sunrise Time|Ambient Pre-Sunset Time|Ambient Post-Sunset Time|Fog Pre-Sunrise Time|Fog Post-Sunrise Time|Fog Pre-Sunset Time|Fog Post-Sunset Time|Sun Pre-Sunrise Time|Sun Post-Sunrise Time|Sun Pre-Sunset Time|Sun Post-Sunset Time|Stars Post-Sunset Start|Stars Pre-Sunrise Finish|Stars Fading Duration|Snow Ripples|Snow Ripple Radius|Snow Ripples Per Flake|Snow Ripple Scale|Snow Ripple Speed|Snow Gravity Scale|Snow High Kill|Snow Low Kill"],
  ["Weather Clear", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Ambient Loop Sound ID"],
  ["Weather Cloudy", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Ambient Loop Sound ID"],
  ["Weather Foggy", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Ambient Loop Sound ID"],
  ["Weather Thunderstorm", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Rain Loop Sound ID|Using Precip|Rain Diameter|Rain Height Min|Rain Height Max|Rain Threshold|Max Raindrops|Rain Entrance Speed|Ambient Loop Sound ID|Flash Decrement"],
  ["Weather Rain", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Rain Loop Sound ID|Using Precip|Rain Diameter|Rain Height Min|Rain Height Max|Rain Threshold|Rain Entrance Speed|Ambient Loop Sound ID|Max Raindrops"],
  ["Weather Overcast", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Ambient Loop Sound ID"],
  ["Weather Ashstorm", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Ambient Loop Sound ID|Storm Threshold"],
  ["Weather Blight", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Ambient Loop Sound ID|Storm Threshold|Disease Chance"],
  ["Weather Snow", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Snow Diameter|Snow Height Min|Snow Height Max|Snow Entrance Speed|Max Snowflakes|Ambient Loop Sound ID|Snow Threshold"],
  ["Weather Blizzard", "Cloud Texture|Clouds Maximum Percent|Transition Delta|Sky Sunrise Color|Sky Day Color|Sky Sunset Color|Sky Night Color|Fog Sunrise Color|Fog Day Color|Fog Sunset Color|Fog Night Color|Ambient Sunrise Color|Ambient Day Color|Ambient Sunset Color|Ambient Night Color|Sun Sunrise Color|Sun Day Color|Sun Sunset Color|Sun Night Color|Sun Disc Sunset Color|Land Fog Day Depth|Land Fog Night Depth|Wind Speed|Cloud Speed|Glare View|Ambient Loop Sound ID|Storm Threshold"],
  ["Moons", "Secunda Size|Secunda Axis Offset|Secunda Speed|Secunda Daily Increment|Secunda Moon Shadow Early Fade Angle|Secunda Fade Start Angle|Secunda Fade End Angle|Secunda Fade In Start|Secunda Fade In Finish|Secunda Fade Out Start|Secunda Fade Out Finish|Masser Size|Masser Axis Offset|Masser Speed|Masser Daily Increment|Masser Moon Shadow Early Fade Angle|Masser Fade Start Angle|Masser Fade End Angle|Masser Fade In Start|Masser Fade In Finish|Masser Fade Out Start|Masser Fade Out Finish|Script Color"],
  ["General", "Werewolf FOV"],
];
let FALLBACK_KEYS = null;

// Morrowind.ini, as a player's might read, and what dream-ini writes for each line. `out` lists
// the cfg lines in the order they are written; `drop` is a line the parser or the importer skips.
const SCRIPT = [
  { ini: '[Game Files]', header: 'Game Files' },
  { ini: 'GameFile0=Morrowind.esm', out: ['content=Morrowind.esm'] },
  { ini: 'GameFile1=Tribunal.esm', out: ['content=Tribunal.esm'] },
  { ini: 'GameFile2=Bloodmoon.esm', out: ['content=Bloodmoon.esm'] },
  { ini: '[Archives]', header: 'Archives' },
  { ini: 'Archive 0=Tribunal.bsa', out: ['fallback-archive=Morrowind.bsa', 'fallback-archive=Tribunal.bsa'] },
  { ini: 'Archive 1=Bloodmoon.bsa', out: ['fallback-archive=Bloodmoon.bsa'] },
  { ini: '[General]', header: 'General' },
  { ini: 'Disable Audio=0', out: ['no-sound=0'] },
  { ini: '[Fonts]', header: 'Fonts' },
  { ini: 'Font 0=magic_cards_regular', drop: true },
  { ini: '[Weather]', header: 'Weather' },
  { ini: 'Sunrise Time=6', out: ['fallback=Weather_Sunrise_Time,6'] },
  { ini: 'Sunset Time=18', out: ['fallback=Weather_Sunset_Time,18'] },
  { ini: '[Weather Clear]', header: 'Weather Clear' },
  { ini: 'Cloud Texture=Tx_Sky_Clear.dds', out: ['fallback=Weather_Clear_Cloud_Texture,Tx_Sky_Clear.dds'] },
  { ini: '; Blight storms are next', drop: true },
  { ini: '[General]', header: 'General' },
  { ini: 'Werewolf FOV=150', out: ['fallback=General_Werewolf_FOV,150'] },
];
// What the cfg holds before an import. dream-ini keeps it as it was, comments included.
const KEPT = ['# openmw.cfg', 'data="C:/Games/Morrowind/Data Files"'];
const ENCODING = 'encoding=win1252';

// The scene, in units where the medallion's radius is 1. The composition spans COMP around its
// centre, and the layout fits that box beside the hero's text.
const COMP = { width: 5.25, height: 2.64, x: 0, y: 0.11 };
const FACE_Z = 0.045;
const SOURCE = { x: -1.95, y: 0.22, z: -0.15, turn: 0.3, width: 1.28, height: 1.72 };
const LEDGER = { x: 1.97, y: -0.12, z: -0.15, turn: -0.3, width: 1.32, height: 1.72 };
const PAGE_PIXELS = { width: 512, height: 688, header: 52, line: 29 };
const PLAQUE = { x: 0, y: 1.2, z: 0.3 };
const CARD_HEIGHT = 0.2;
// The still covers the composition's box with a margin: the cord and the halo reach past it.
const STILL_MARGIN = 1.18;
const ATLAS = { width: 1024, rows: 16, row: 32 };

const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
const stillCapture = new URLSearchParams(location.search).has('dini-still');

// Helpers ----------------------------------------------------------------------------------------

function fallbackKeys() {
  if (FALLBACK_KEYS) return FALLBACK_KEYS;
  FALLBACK_KEYS = [];
  for (const [section, keys] of FALLBACK_SECTIONS) for (const key of keys.split('|')) FALLBACK_KEYS.push([section, key]);
  return FALLBACK_KEYS;
}

function cssValue(name, fallback) {
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return raw || fallback;
}

function cssColor(name, fallback) {
  const color = new THREE.Color(fallback);
  const raw = cssValue(name, '');
  if (raw) {
    try { color.setStyle(raw); } catch { /* an unparsable token keeps the fallback */ }
  }
  return color;
}

function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function byte(value) {
  return Math.max(0, Math.min(255, Math.round(value * 255)));
}

function canvas2d(width, height) {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  return [canvas, canvas.getContext('2d', { willReadFrequently: false })];
}

function texture(canvas, colorSpace, anisotropy) {
  const result = new THREE.CanvasTexture(canvas);
  result.colorSpace = colorSpace;
  result.anisotropy = anisotropy;
  return result;
}

function normalMap(heightCanvas, strength) {
  const { width, height } = heightCanvas;
  const source = heightCanvas.getContext('2d').getImageData(0, 0, width, height).data;
  const [canvas, context] = canvas2d(width, height);
  const image = context.createImageData(width, height);
  const out = image.data;
  const h = (x, y) => source[(Math.min(height - 1, Math.max(0, y)) * width + Math.min(width - 1, Math.max(0, x))) * 4] / 255;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const dx = (h(x + 1, y) - h(x - 1, y)) * strength;
      const dy = (h(x, y + 1) - h(x, y - 1)) * strength;
      const inverse = 1 / Math.hypot(dx, dy, 1);
      const i = (y * width + x) * 4;
      out[i] = byte(-dx * inverse * 0.5 + 0.5);
      out[i + 1] = byte(dy * inverse * 0.5 + 0.5);
      out[i + 2] = byte(inverse * 0.5 + 0.5);
      out[i + 3] = 255;
    }
  }
  context.putImageData(image, 0, 0);
  return canvas;
}

// The traces -------------------------------------------------------------------------------------

// The icon's routing: in each quadrant two bundles of five traces leave the diamond around the
// ring, run out along an axis, jog 45 degrees and run on to a pad near the rim. The bundles nest,
// so no two traces cross. Distances run from the inner end, where the ring is, to the pad.
function generateTraces() {
  const rim = 0.885;
  const traces = [];
  for (const [sx, sy] of [[1, 1], [-1, 1], [-1, -1], [1, -1]]) {
    for (const swap of [false, true]) {
      for (let k = 0; k < 5; k++) {
        const x0 = 0.026 + 0.028 * k;
        const x2 = x0 + 0.21;
        const local = [[x0, 0.3 - x0], [x0, 0.62 - x0], [x2, 1.04 - x2], [x2, Math.sqrt(rim * rim - x2 * x2)]];
        const points = local.map(([x, y]) => (swap ? [y * sx, x * sy] : [x * sx, y * sy]));
        const lengths = [0];
        for (let i = 1; i < points.length; i++) lengths.push(lengths[i - 1] + Math.hypot(points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1]));
        const pad = points[points.length - 1];
        traces.push({ points, lengths, length: lengths[lengths.length - 1], pad, input: pad[0] < 0, via: k % 2 === 0 ? points[1] : null, busy: 0 });
      }
    }
  }
  return traces;
}

function pointOnTrace(trace, distance, target) {
  const { points, lengths } = trace;
  const d = Math.max(0, Math.min(trace.length, distance));
  let i = 1;
  while (i < lengths.length - 1 && lengths[i] < d) i++;
  const span = Math.max(1e-6, lengths[i] - lengths[i - 1]);
  const f = (d - lengths[i - 1]) / span;
  target.x = points[i - 1][0] + (points[i][0] - points[i - 1][0]) * f;
  target.y = points[i - 1][1] + (points[i][1] - points[i - 1][1]) * f;
  return target;
}

// The board's maps, baked on canvases: colour, height (for the normal map), surface (ambient
// occlusion, roughness, metalness) and flow (trace mask, distance along the trace, a per-trace
// offset), which the face's shader turns into light running along the traces.
function bakeBoard(size, traces, mono, anisotropy) {
  const unit = size / 2;
  const px = (x) => (x + 1) * unit;
  const py = (y) => (1 - y) * unit;
  const next = random(1820);
  const [colorCanvas, color] = canvas2d(size, size);
  const [heightCanvas, height] = canvas2d(size, size);
  const [surfaceCanvas, surface] = canvas2d(size, size);
  const [flowCanvas, flow] = canvas2d(size, size);

  // The substrate: black lacquer over a fibreglass weave.
  const [tile, tileContext] = canvas2d(64, 64);
  const tileImage = tileContext.createImageData(64, 64);
  for (let y = 0; y < 64; y++) {
    for (let x = 0; x < 64; x++) {
      const weave = ((Math.floor(x / 8) + Math.floor(y / 8)) % 2 ? Math.sin((x % 8) / 8 * Math.PI) : Math.sin((y % 8) / 8 * Math.PI)) * 0.5;
      const grain = next() * 0.35;
      const i = (y * 64 + x) * 4;
      tileImage.data[i] = 14 + weave * 9 + grain * 6;
      tileImage.data[i + 1] = 10 + weave * 6 + grain * 4;
      tileImage.data[i + 2] = 8 + weave * 4 + grain * 3;
      tileImage.data[i + 3] = 255;
    }
  }
  tileContext.putImageData(tileImage, 0, 0);
  color.fillStyle = color.createPattern(tile, 'repeat');
  color.fillRect(0, 0, size, size);
  const [heightTile, heightTileContext] = canvas2d(64, 64);
  const heightTileImage = heightTileContext.createImageData(64, 64);
  for (let i = 0; i < 64 * 64; i++) {
    const v = 18 + (tileImage.data[i * 4] - 14) * 1.2;
    heightTileImage.data[i * 4] = heightTileImage.data[i * 4 + 1] = heightTileImage.data[i * 4 + 2] = v;
    heightTileImage.data[i * 4 + 3] = 255;
  }
  heightTileContext.putImageData(heightTileImage, 0, 0);
  height.fillStyle = height.createPattern(heightTile, 'repeat');
  height.fillRect(0, 0, size, size);
  surface.fillStyle = 'rgb(255, 72, 0)';
  surface.fillRect(0, 0, size, size);
  flow.fillStyle = '#000';
  flow.fillRect(0, 0, size, size);

  const layers = [
    { context: color, trace: '#d9ac55', pad: '#e8c26a', hole: '#060403', silk: '#e9dcc0', copper: '#4a3212' },
    { context: height, trace: '#b0b0b0', pad: '#c8c8c8', hole: '#000000', silk: '#3a3a3a', copper: '#6a6a6a' },
    { context: surface, trace: 'rgb(255, 88, 235)', pad: 'rgb(255, 58, 255)', hole: 'rgb(70, 200, 0)', silk: 'rgb(255, 205, 0)', copper: 'rgb(210, 150, 160)' },
  ];
  const traceWidth = 0.0105 * unit;
  for (const layer of layers) {
    const c = layer.context;
    c.lineJoin = 'round';
    c.lineCap = 'round';
    // The rings: the rim's edge, the ground ring inside it, and the diamond around the parser.
    c.strokeStyle = layer.copper;
    c.lineWidth = 0.02 * unit;
    c.beginPath();
    c.arc(unit, unit, 0.975 * unit, 0, Math.PI * 2);
    c.stroke();
    c.lineWidth = traceWidth * 0.8;
    c.beginPath();
    c.arc(unit, unit, 0.93 * unit, 0, Math.PI * 2);
    c.stroke();
    c.beginPath();
    c.moveTo(px(0), py(0.29));
    c.lineTo(px(0.29), py(0));
    c.lineTo(px(0), py(-0.29));
    c.lineTo(px(-0.29), py(0));
    c.closePath();
    c.stroke();
    c.lineWidth = 0.03 * unit;
    c.beginPath();
    c.arc(unit, unit, 0.175 * unit, 0, Math.PI * 2);
    c.stroke();
    // The traces: a copper bed, and the plated trace on it, as the icon draws them outlined.
    for (const [style, widthScale] of [[layer.copper, 1.35], [layer.trace, 0.72]]) {
      c.strokeStyle = style;
      c.lineWidth = traceWidth * widthScale;
      for (const trace of traces) {
        c.beginPath();
        trace.points.forEach(([x, y], i) => (i ? c.lineTo(px(x), py(y)) : c.moveTo(px(x), py(y))));
        c.stroke();
      }
    }
    // Pads with drill holes at each trace's outer end, vias on alternate traces.
    for (const trace of traces) {
      for (const [point, radius, hole] of [[trace.pad, 0.024, 0.009], [trace.via, 0.016, 0.006]]) {
        if (!point) continue;
        c.fillStyle = layer.pad;
        c.beginPath();
        c.arc(px(point[0]), py(point[1]), radius * unit, 0, Math.PI * 2);
        c.fill();
        c.fillStyle = layer.hole;
        c.beginPath();
        c.arc(px(point[0]), py(point[1]), hole * unit, 0, Math.PI * 2);
        c.fill();
      }
    }
    // Silkscreen: the ring's legend, and the pads' names.
    c.fillStyle = layer.silk;
    c.textAlign = 'center';
    c.textBaseline = 'middle';
    c.font = `600 ${Math.round(0.032 * unit)}px ${mono}`;
    const legend = 'MORROWIND.INI \u2192 OPENMW.CFG \u00b7 WIN1252 \u00b7 573 KEYS \u00b7 FALLBACK= \u00b7 CONTENT= \u00b7 DREAM-INI \u00b7 ';
    const around = legend.repeat(2);
    const step = Math.PI * 2 / around.length;
    for (let i = 0; i < around.length; i++) {
      const angle = Math.PI / 2 - i * step;
      c.save();
      c.translate(px(Math.cos(angle) * 0.953), py(Math.sin(angle) * 0.953));
      c.rotate(-angle + Math.PI / 2);
      c.fillText(around[i], 0, 0);
      c.restore();
    }
    c.font = `600 ${Math.round(0.022 * unit)}px ${mono}`;
    const labels = { input: ['GF0', 'GF1', 'GF2', 'AR0', 'AR1'], output: ['C=', 'FA=', 'FB=', 'ENC', 'NS='] };
    traces.forEach((trace, index) => {
      const names = trace.input ? labels.input : labels.output;
      const [x, y] = trace.pad;
      const r = Math.hypot(x, y);
      if (index % 3) return;
      c.fillText(names[index % names.length], px(x - x / r * 0.055), py(y - y / r * 0.055));
    });
  }

  // Flow: each trace drawn in short steps, green carrying the distance from its inner end.
  flow.lineCap = 'round';
  flow.lineWidth = traceWidth * 0.9;
  const target = { x: 0, y: 0 };
  const previous = { x: 0, y: 0 };
  traces.forEach((trace) => {
    const offset = next();
    const steps = Math.ceil(trace.length / 0.008);
    pointOnTrace(trace, 0, previous);
    for (let i = 1; i <= steps; i++) {
      const d = trace.length * i / steps;
      pointOnTrace(trace, d, target);
      flow.strokeStyle = `rgb(255, ${byte(d / 1.25)}, ${byte(offset)})`;
      flow.beginPath();
      flow.moveTo(px(previous.x), py(previous.y));
      flow.lineTo(px(target.x), py(target.y));
      flow.stroke();
      previous.x = target.x;
      previous.y = target.y;
    }
    flow.fillStyle = `rgb(255, ${byte(trace.length / 1.25)}, ${byte(offset)})`;
    flow.beginPath();
    flow.arc(px(trace.pad[0]), py(trace.pad[1]), 0.02 * unit, 0, Math.PI * 2);
    flow.fill();
  });

  // Soften the height map so the traces are rounded, then derive the normals from it.
  const [soft, softContext] = canvas2d(size, size);
  softContext.filter = `blur(${Math.max(1, size / 1024)}px)`;
  softContext.drawImage(heightCanvas, 0, 0);
  return {
    map: texture(colorCanvas, THREE.SRGBColorSpace, anisotropy),
    normal: texture(normalMap(soft, size / 512), THREE.NoColorSpace, anisotropy),
    surface: texture(surfaceCanvas, THREE.NoColorSpace, anisotropy),
    flow: texture(flowCanvas, THREE.NoColorSpace, 1),
  };
}

// The cord: three strands laid around the rim like the icon's braid, each a tube spiralling about
// the ring's centre line, in the icon's leather scales.
class Strand extends THREE.Curve {
  constructor(radius, amplitude, twists, phase) {
    super();
    Object.assign(this, { radius, amplitude, twists, phase });
  }

  getPoint(t, target = new THREE.Vector3()) {
    const angle = t * Math.PI * 2;
    const turn = angle * this.twists + this.phase;
    const r = this.radius + Math.cos(turn) * this.amplitude;
    return target.set(Math.cos(angle) * r, Math.sin(angle) * r, Math.sin(turn) * this.amplitude);
  }
}

function braidMaps(anisotropy) {
  const width = 256;
  const height = 64;
  const [colorCanvas, color] = canvas2d(width, height);
  const [heightCanvas, bump] = canvas2d(width, height);
  color.fillStyle = '#2a140a';
  color.fillRect(0, 0, width, height);
  bump.fillStyle = '#000';
  bump.fillRect(0, 0, width, height);
  // Two rows of overlapping scales, offset by half a scale, like the icon's plaited leather.
  for (let row = 0; row < 2; row++) {
    for (let i = -1; i < 5; i++) {
      const cx = i * 64 + (row ? 32 : 0);
      const cy = row ? 44 : 20;
      for (const [context, fill, edge] of [[color, null, '#170904'], [bump, null, '#000']]) {
        const gradient = context.createLinearGradient(cx - 30, cy, cx + 30, cy);
        if (context === color) {
          gradient.addColorStop(0, '#5a2e14');
          gradient.addColorStop(0.55, '#9a5a2c');
          gradient.addColorStop(1, '#4a220e');
        } else {
          gradient.addColorStop(0, '#303030');
          gradient.addColorStop(0.55, '#e0e0e0');
          gradient.addColorStop(1, '#404040');
        }
        context.fillStyle = fill || gradient;
        context.strokeStyle = edge;
        context.lineWidth = 3;
        context.beginPath();
        context.moveTo(cx - 32, cy - 10);
        context.quadraticCurveTo(cx, cy - 26, cx + 32, cy - 10);
        context.lineTo(cx + 26, cy + 14);
        context.quadraticCurveTo(cx, cy + 4, cx - 26, cy + 14);
        context.closePath();
        context.fill();
        context.stroke();
      }
    }
  }
  const map = texture(colorCanvas, THREE.SRGBColorSpace, anisotropy);
  const normal = texture(normalMap(heightCanvas, 3), THREE.NoColorSpace, anisotropy);
  for (const t of [map, normal]) {
    t.wrapS = t.wrapT = THREE.RepeatWrapping;
    t.repeat.set(70, 1);
  }
  return { map, normal };
}

// A small room of warm light panels, filtered into the environment the gold reflects.
function environment(renderer, accent) {
  const scene = new THREE.Scene();
  const disposables = [];
  const add = (geometry, color, position) => {
    const material = new THREE.MeshBasicMaterial({ color, side: THREE.DoubleSide });
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set(...position);
    mesh.lookAt(0, 0, 0);
    scene.add(mesh);
    disposables.push(geometry, material);
  };
  const room = new THREE.Mesh(new THREE.BoxGeometry(26, 16, 26), new THREE.MeshBasicMaterial({ color: new THREE.Color(0.035, 0.028, 0.02), side: THREE.BackSide }));
  scene.add(room);
  disposables.push(room.geometry, room.material);
  add(new THREE.PlaneGeometry(10, 7), new THREE.Color(1.0, 0.84, 0.6).multiplyScalar(3.4), [-6, 6, 8]);
  add(new THREE.PlaneGeometry(7, 5), new THREE.Color(0.95, 0.92, 0.88).multiplyScalar(1.8), [7, 2, 8]);
  add(new THREE.PlaneGeometry(14, 0.6), accent.clone().multiplyScalar(5), [0, 7, -6]);
  add(new THREE.PlaneGeometry(0.6, 12), new THREE.Color(1.0, 0.7, 0.4).multiplyScalar(3), [-9, 0, -4]);
  add(new THREE.PlaneGeometry(0.5, 10), new THREE.Color(0.7, 0.8, 1.0).multiplyScalar(2.4), [9, 0, -5]);
  add(new THREE.PlaneGeometry(24, 24), new THREE.Color(0.3, 0.2, 0.08).multiplyScalar(0.4), [0, -7.5, 0]);
  const generator = new THREE.PMREMGenerator(renderer);
  const target = generator.fromScene(scene, 0.03);
  generator.dispose();
  for (const item of disposables) item.dispose();
  return target;
}

// Shaders ----------------------------------------------------------------------------------------

const SCRUB = /* glsl */ `
  vec3 scrub(vec3 c) {
    if (any(isnan(c)) || any(isinf(c)) || c.r != c.r || c.g != c.g || c.b != c.b) return vec3(0.0);
    return clamp(c, 0.0, 64.0);
  }
  float scrub1(float v) {
    if (isnan(v) || isinf(v) || v != v) return 0.0;
    return clamp(v, 0.0, 64.0);
  }
`;

const NOISE = /* glsl */ `
  float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
  float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
  }
`;

const FULLSCREEN_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = vec4(position.xy, 0.0, 1.0);
  }
`;

const PLANE_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const BRIGHT_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform float uThreshold;
  varying vec2 vUv;
  ${SCRUB}
  void main() {
    vec3 c = scrub(texture2D(tInput, vUv).rgb);
    float luma = dot(c, vec3(0.2126, 0.7152, 0.0722));
    gl_FragColor = vec4(c * smoothstep(uThreshold, uThreshold + 0.6, luma), 1.0);
  }
`;

const BLUR_FRAGMENT = /* glsl */ `
  uniform sampler2D tInput;
  uniform vec2 uDirection;
  varying vec2 vUv;
  void main() {
    vec3 sum = texture2D(tInput, vUv).rgb * 0.2270270270;
    sum += texture2D(tInput, vUv + uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv - uDirection * 1.3846153846).rgb * 0.3162162162;
    sum += texture2D(tInput, vUv + uDirection * 3.2307692308).rgb * 0.0702702703;
    sum += texture2D(tInput, vUv - uDirection * 3.2307692308).rgb * 0.0702702703;
    gl_FragColor = vec4(sum, 1.0);
  }
`;

// The scene holds premultiplied colour and coverage. Light with no coverage (bloom, sparks, the
// flow's glow) is given the coverage of its own brightness, so it lays over the page's
// background as light does.
const COMPOSITE_FRAGMENT = /* glsl */ `
  uniform sampler2D tScene;
  uniform sampler2D tBloomNear;
  uniform sampler2D tBloomFar;
  uniform float uTime;
  uniform float uExposure;
  varying vec2 vUv;
  vec3 aces(vec3 x) {
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
  }
  float dither(vec2 p) {
    return fract(sin(dot(p + fract(uTime), vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
  }
  ${SCRUB}
  void main() {
    vec4 scene = texture2D(tScene, vUv);
    vec3 color = scrub(scene.rgb);
    float coverage = clamp(scrub1(scene.a), 0.0, 1.0);
    color += scrub(texture2D(tBloomNear, vUv).rgb) * 0.62 + scrub(texture2D(tBloomFar, vUv).rgb) * 0.5;
    vec3 mapped = pow(aces(color * uExposure), vec3(1.0 / 2.2));
    float light = max(mapped.r, max(mapped.g, mapped.b));
    float alpha = max(coverage, clamp(light * 1.15, 0.0, 1.0));
    mapped += dither(gl_FragCoord.xy) / 255.0;
    mapped = clamp(mapped, 0.0, alpha);
    gl_FragColor = vec4(mapped, alpha);
  }
`;

// The parser: a lens in the central ring that ripples with every line it reads.
const LENS_FRAGMENT = /* glsl */ `
  uniform float uTime;
  uniform float uPulse;
  uniform float uActivity;
  uniform vec3 uColor;
  varying vec2 vUv;
  ${SCRUB}
  ${NOISE}
  void main() {
    vec2 p = vUv * 2.0 - 1.0;
    float r = length(p);
    float angle = atan(p.y, p.x);
    float swirl = noise(vec2(angle * 1.5 + uTime * 0.6, r * 6.0 - uTime * 1.2));
    float ripples = 0.5 + 0.5 * sin(r * 30.0 - uTime * 4.0 - uActivity * uTime * 6.0);
    float core = exp(-r * r * 5.0);
    float front = exp(-abs(r - (1.0 - uPulse)) * 14.0) * uPulse;
    vec3 c = uColor * (core * (0.5 + uActivity * 2.2) + ripples * 0.16 * (1.0 - r) * (0.6 + swirl) + front * 3.0);
    c += vec3(1.0, 0.95, 0.85) * core * core * (0.3 + uActivity * 1.5 + uPulse * 1.2);
    float edge = 1.0 - smoothstep(0.93, 1.0, r);
    gl_FragColor = vec4(scrub(c) * edge, edge);
  }
`;

// A card: dark glass with a gold edge and its line of text; it can burn away.
const CARD_FRAGMENT = /* glsl */ `
  uniform sampler2D tAtlas;
  uniform vec4 uRect;
  uniform vec2 uPad;
  uniform float uAspect;
  uniform float uOpacity;
  uniform float uDissolve;
  uniform float uFlash;
  uniform vec3 uTint;
  uniform vec3 uEdge;
  varying vec2 vUv;
  ${SCRUB}
  ${NOISE}
  void main() {
    vec2 inner = (vUv - uPad) / max(vec2(1e-4), 1.0 - 2.0 * uPad);
    float text = 0.0;
    if (inner.x > 0.0 && inner.x < 1.0 && inner.y > 0.0 && inner.y < 1.0) {
      text = texture2D(tAtlas, vec2(mix(uRect.x, uRect.z, inner.x), mix(uRect.y, uRect.w, inner.y))).a;
    }
    float ex = min(vUv.x, 1.0 - vUv.x) * uAspect;
    float ey = min(vUv.y, 1.0 - vUv.y);
    float e = min(ex, ey);
    float border = 1.0 - smoothstep(0.03, 0.07, e);
    float burn = noise(vec2(vUv.x * uAspect * 7.0, vUv.y * 7.0));
    if (burn < uDissolve) discard;
    float ember = 1.0 - smoothstep(0.0, 0.08, burn - uDissolve);
    ember *= step(0.001, uDissolve);
    vec3 glass = vec3(0.035, 0.025, 0.015);
    vec3 c = glass + uEdge * border * 1.6 + uTint * text * (1.6 + uFlash * 3.0) + vec3(1.0, 0.55, 0.2) * ember * 4.0;
    float alpha = max(0.62, max(border, text)) * uOpacity;
    gl_FragColor = vec4(scrub(c) * alpha, alpha);
  }
`;

// A page: its canvas already carries the panel, the frame and the text; bright text blooms.
const PAGE_FRAGMENT = /* glsl */ `
  uniform sampler2D tPage;
  uniform float uGlow;
  uniform float uOpacity;
  varying vec2 vUv;
  ${SCRUB}
  void main() {
    vec4 t = texture2D(tPage, vUv);
    float luma = dot(t.rgb, vec3(0.2126, 0.7152, 0.0722));
    vec3 c = t.rgb * (1.0 + uGlow * luma * luma * 3.0);
    float alpha = t.a * uOpacity;
    gl_FragColor = vec4(scrub(c) * alpha, alpha);
  }
`;

// Light: packets on the traces and sparks from dropped lines. They add light and no coverage.
const POINT_VERTEX = /* glsl */ `
  attribute float aSize;
  attribute float aAlpha;
  attribute vec3 aColor;
  uniform float uPixel;
  uniform float uScale;
  varying float vAlpha;
  varying vec3 vColor;
  void main() {
    vec4 view = modelViewMatrix * vec4(position, 1.0);
    gl_Position = projectionMatrix * view;
    gl_PointSize = clamp(aSize * uScale * uPixel / max(0.05, -view.z), 0.0, 96.0);
    vAlpha = aAlpha;
    vColor = aColor;
  }
`;

const POINT_FRAGMENT = /* glsl */ `
  varying float vAlpha;
  varying vec3 vColor;
  ${SCRUB}
  void main() {
    vec2 p = gl_PointCoord * 2.0 - 1.0;
    float r2 = dot(p, p);
    if (r2 > 1.0 || vAlpha <= 0.0) discard;
    float light = exp(-r2 * 5.0) + exp(-r2 * 40.0) * 1.5;
    gl_FragColor = vec4(scrub(vColor * light * vAlpha), 0.0);
  }
`;

// Motes: the INI's own punctuation, [ ] = and ;, drifting in the warm dark.
const MOTE_VERTEX = /* glsl */ `
  attribute vec4 aSeed;
  uniform float uTime;
  uniform float uPixel;
  uniform float uScale;
  uniform vec3 uCenter;
  uniform vec2 uBox;
  varying float vGlyph;
  varying float vAlpha;
  void main() {
    float rise = fract(aSeed.y + uTime * (0.006 + aSeed.w * 0.01));
    vec3 p = vec3(
      (aSeed.x - 0.5) * uBox.x + sin(uTime * 0.2 + aSeed.z * 6.28) * 0.12,
      (rise - 0.5) * uBox.y,
      (aSeed.z - 0.6) * 2.4
    );
    vec4 view = modelViewMatrix * vec4(uCenter + p * uScale, 1.0);
    gl_Position = projectionMatrix * view;
    gl_PointSize = clamp((0.04 + aSeed.w * 0.04) * uScale * uPixel / max(0.05, -view.z), 0.0, 40.0);
    vGlyph = floor(aSeed.x * 97.0) - 4.0 * floor(floor(aSeed.x * 97.0) / 4.0);
    vAlpha = smoothstep(0.0, 0.15, rise) * (1.0 - smoothstep(0.8, 1.0, rise)) * (0.1 + aSeed.w * 0.22);
  }
`;

const MOTE_FRAGMENT = /* glsl */ `
  uniform sampler2D tGlyphs;
  uniform vec3 uColor;
  varying float vGlyph;
  varying float vAlpha;
  ${SCRUB}
  void main() {
    vec2 uv = vec2((vGlyph + gl_PointCoord.x) / 4.0, 1.0 - gl_PointCoord.y);
    float a = texture2D(tGlyphs, uv).a * vAlpha;
    if (a <= 0.002) discard;
    gl_FragColor = vec4(scrub(uColor * a), 0.0);
  }
`;

// The backlight: a soft glow with slow rays behind the board, swelling as it parses.
const HALO_FRAGMENT = /* glsl */ `
  uniform float uTime;
  uniform float uGain;
  uniform vec3 uColor;
  varying vec2 vUv;
  ${SCRUB}
  ${NOISE}
  void main() {
    vec2 p = vUv * 2.0 - 1.0;
    float r = length(p);
    float angle = atan(p.y, p.x);
    float rays = noise(vec2(angle * 7.0 + uTime * 0.15, uTime * 0.2)) * noise(vec2(angle * 13.0 - uTime * 0.1, 3.0));
    float glow = exp(-r * r * 9.0) * 0.7 + exp(-r * r * 3.2) * 0.12;
    float shafts = rays * exp(-r * 2.6) * smoothstep(0.25, 0.45, r) * 1.1;
    float fade = 1.0 - smoothstep(0.55, 1.0, r);
    gl_FragColor = vec4(scrub(uColor * (glow + shafts) * uGain * fade), 0.0);
  }
`;

// The key halo: fallback keys written round two rings about the board, as a legend of the 573.
const KEYRING_FRAGMENT = /* glsl */ `
  uniform sampler2D tKeys;
  uniform float uGain;
  uniform vec3 uColor;
  varying vec2 vUv;
  ${SCRUB}
  void main() {
    float a = texture2D(tKeys, vUv).a;
    gl_FragColor = vec4(scrub(uColor * a * uGain), 0.0);
  }
`;

// The finale's shockwave: a ring of light running out from the board.
const WAVE_FRAGMENT = /* glsl */ `
  uniform float uAge;
  uniform vec3 uColor;
  varying vec2 vUv;
  ${SCRUB}
  void main() {
    vec2 p = vUv * 2.0 - 1.0;
    float r = length(p);
    float radius = 0.15 + uAge * 0.85;
    float ring = exp(-abs(r - radius) * 28.0) * (1.0 - uAge) * (1.0 - uAge);
    float fade = 1.0 - smoothstep(0.92, 1.0, r);
    gl_FragColor = vec4(scrub(uColor * ring * fade * 3.0), 0.0);
  }
`;

function fullscreenMaterial(fragmentShader, uniforms) {
  return new THREE.ShaderMaterial({ vertexShader: FULLSCREEN_VERTEX, fragmentShader, uniforms, depthTest: false, depthWrite: false });
}

const PREMULTIPLIED = { transparent: true, depthWrite: false, blending: THREE.CustomBlending, blendSrc: THREE.OneFactor, blendDst: THREE.OneMinusSrcAlphaFactor, blendSrcAlpha: THREE.OneFactor, blendDstAlpha: THREE.OneMinusSrcAlphaFactor };
const LIGHT = { transparent: true, depthWrite: false, blending: THREE.CustomBlending, blendSrc: THREE.OneFactor, blendDst: THREE.OneFactor, blendSrcAlpha: THREE.ZeroFactor, blendDstAlpha: THREE.OneFactor };

// Light runs along the traces: in towards the ring on the left, where the INI comes in, and out
// to the pads on the right, where the cfg leaves.
function withFlow(material, flow, uniforms) {
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms, { uFlowMap: { value: flow } });
    shader.fragmentShader = shader.fragmentShader
      .replace('#include <common>', '#include <common>\nuniform sampler2D uFlowMap;\nuniform float uFlowTime;\nuniform float uFlowGain;\nuniform float uFlowBase;\nuniform vec3 uFlowColor;')
      .replace('#include <emissivemap_fragment>', `#include <emissivemap_fragment>
        vec3 flowSample = texture2D(uFlowMap, vMapUv).rgb;
        float flowMask = flowSample.r;
        float flowDistance = flowSample.g / max(flowMask, 0.02);
        float flowOffset = flowSample.b / max(flowMask, 0.02);
        float flowOut = step(0.5, vMapUv.x) * 2.0 - 1.0;
        float flowPhase = fract(flowDistance * 9.0 - uFlowTime * flowOut + flowOffset);
        float flowBand = smoothstep(0.0, 0.05, flowPhase) * (1.0 - smoothstep(0.05, 0.42, flowPhase));
        totalEmissiveRadiance = uFlowColor * flowMask * (uFlowBase + flowBand * uFlowGain);`);
  };
  material.customProgramCacheKey = () => 'dream-ini-flow';
  return material;
}

// Text surfaces ----------------------------------------------------------------------------------

// One canvas holds a row of text for every card in flight.
function createAtlas(mono) {
  const [canvas, context] = canvas2d(ATLAS.width, ATLAS.rows * ATLAS.row);
  const atlas = texture(canvas, THREE.NoColorSpace, 4);
  atlas.minFilter = THREE.LinearMipmapLinearFilter;
  const font = `600 ${ATLAS.row * 0.62}px ${mono}`;
  return {
    texture: atlas,
    write(row, text) {
      context.clearRect(0, row * ATLAS.row, ATLAS.width, ATLAS.row);
      context.font = font;
      context.textBaseline = 'middle';
      context.fillStyle = '#fff';
      const width = Math.min(ATLAS.width - 8, Math.ceil(context.measureText(text).width) + 4);
      context.fillText(text, 2, row * ATLAS.row + ATLAS.row / 2 + 1, ATLAS.width - 8);
      atlas.needsUpdate = true;
      // uv rectangle, with v up (the canvas is flipped on upload).
      const top = 1 - row / ATLAS.rows;
      const bottom = 1 - (row + 1) / ATLAS.rows;
      return { rect: new THREE.Vector4(0, bottom, width / ATLAS.width, top), aspect: width / ATLAS.row };
    },
  };
}

// The INI and the cfg, as pages drawn on canvases.
function createPage(kind, mono, accent, text, small) {
  const scale = small ? 0.5 : 1;
  const width = Math.round(PAGE_PIXELS.width * scale);
  const height = Math.round(PAGE_PIXELS.height * scale);
  const [canvas, context] = canvas2d(width, height);
  const pageTexture = texture(canvas, THREE.SRGBColorSpace, 4);
  const gold = `#${accent.getHexString()}`;
  const ink = `#${text.getHexString()}`;
  const s = (value) => value * scale;
  const lineHeight = s(PAGE_PIXELS.line);
  const header = s(PAGE_PIXELS.header);
  const visible = Math.floor((height - header - s(14)) / lineHeight);
  const title = kind === 'source' ? 'Morrowind.ini' : 'openmw.cfg';

  function frame(badge) {
    context.clearRect(0, 0, width, height);
    const radius = s(14);
    context.beginPath();
    context.roundRect(s(3), s(3), width - s(6), height - s(6), radius);
    context.fillStyle = 'rgba(16, 12, 7, 0.7)';
    context.fill();
    context.lineWidth = s(2.5);
    context.strokeStyle = 'rgba(224, 184, 103, 0.55)';
    context.stroke();
    context.fillStyle = 'rgba(224, 184, 103, 0.1)';
    context.fillRect(s(4), s(4), width - s(8), header - s(6));
    context.font = `700 ${s(22)}px ${mono}`;
    context.textBaseline = 'middle';
    context.fillStyle = gold;
    context.fillText(title, s(20), header / 2 + s(1));
    for (let i = 0; i < 3; i++) {
      context.beginPath();
      context.arc(width - s(26) - i * s(16), header / 2, s(4.5), 0, Math.PI * 2);
      context.fillStyle = i ? 'rgba(224, 184, 103, 0.3)' : 'rgba(224, 184, 103, 0.7)';
      context.fill();
    }
    if (badge) {
      context.font = `600 ${s(15)}px ${mono}`;
      const w = context.measureText(badge).width + s(16);
      context.beginPath();
      context.roundRect(width - s(80) - w, header / 2 - s(12), w, s(24), s(12));
      context.fillStyle = gold;
      context.fill();
      context.fillStyle = '#140e06';
      context.fillText(badge, width - s(72) - w, header / 2 + s(1));
    }
  }

  function line(y, value, style, glow) {
    context.font = `600 ${s(19)}px ${mono}`;
    if (glow > 0) {
      context.fillStyle = `rgba(224, 184, 103, ${0.2 * glow})`;
      context.fillRect(s(8), y - lineHeight / 2 + s(1), width - s(16), lineHeight - s(2));
    }
    const x = s(20);
    if (style === 'header') {
      context.fillStyle = glow > 0 ? '#fff4d6' : gold;
      context.fillText(value, x, y);
      return;
    }
    if (style === 'dim') {
      context.fillStyle = 'rgba(200, 180, 150, 0.35)';
      context.fillText(value, x, y);
      return;
    }
    if (style === 'struck') {
      context.fillStyle = 'rgba(200, 180, 150, 0.3)';
      context.fillText(value, x, y);
      const w = context.measureText(value).width;
      context.fillRect(x, y, w, Math.max(1, s(1.5)));
      return;
    }
    const split = value.indexOf('=');
    const bright = glow > 0 ? '#fff4d6' : ink;
    if (split < 0 || value.startsWith(';') || value.startsWith('#')) {
      context.fillStyle = value.startsWith(';') || value.startsWith('#') ? 'rgba(200, 180, 150, 0.45)' : bright;
      context.fillText(value, x, y);
      return;
    }
    const key = value.slice(0, split);
    context.fillStyle = bright;
    context.fillText(key, x, y);
    const kw = context.measureText(key).width;
    context.fillStyle = gold;
    context.fillText('=', x + kw, y);
    const ew = context.measureText('=').width;
    context.fillStyle = glow > 0 ? '#ffe7b0' : 'rgba(236, 222, 196, 0.72)';
    context.fillText(value.slice(split + 1), x + kw + ew, y);
  }

  return {
    texture: pageTexture,
    visible,
    // rows: [{ text, style, glow }], drawn from `first`; returns nothing.
    draw(rows, first, badge) {
      frame(badge);
      context.save();
      context.beginPath();
      context.rect(0, header, width, height - header - s(8));
      context.clip();
      for (let i = 0; i < visible + 1; i++) {
        const row = rows[first + i];
        if (!row) continue;
        line(header + s(10) + (i + 0.5) * lineHeight, row.text, row.style, row.glow || 0);
      }
      context.restore();
      pageTexture.needsUpdate = true;
    },
    // Where row `i` sits once `first` is at the top, in the page's local units.
    rowPosition(i, first, pageWidth, pageHeight, target) {
      const y = header + s(10) + (i - first + 0.5) * lineHeight;
      target.set(-pageWidth * 0.18, pageHeight * (0.5 - y / height), 0.01);
      return target;
    },
  };
}

// Fallback keys, as dream-ini writes them, around a circle at the given radius (in units of the
// ring's outer radius).
function keyringTexture(mono, keys, radius, size) {
  const [canvas, context] = canvas2d(size, size);
  const half = size / 2;
  context.font = `700 ${Math.round(size * 0.0165)}px ${mono}`;
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  context.fillStyle = '#fff';
  const text = `${keys.join(' \u00b7 ')} \u00b7 `;
  const widths = [...text].map((character) => context.measureText(character).width);
  const total = widths.reduce((a, b) => a + b, 0);
  const circumference = Math.PI * 2 * radius * half;
  const spacing = circumference / total;
  let angle = Math.PI / 2;
  [...text].forEach((character, i) => {
    const step = widths[i] * spacing / (radius * half);
    angle -= step / 2;
    context.save();
    context.translate(half + Math.cos(angle) * radius * half, half - Math.sin(angle) * radius * half);
    context.rotate(-angle + Math.PI / 2);
    context.fillText(character, 0, 0);
    context.restore();
    angle -= step / 2;
  });
  const result = texture(canvas, THREE.NoColorSpace, 4);
  return result;
}

function glyphTexture(mono) {
  const [canvas, context] = canvas2d(256, 64);
  context.font = `700 50px ${mono}`;
  context.textAlign = 'center';
  context.textBaseline = 'middle';
  context.fillStyle = '#fff';
  ['[', ']', '=', ';'].forEach((glyph, i) => context.fillText(glyph, i * 64 + 32, 34));
  return texture(canvas, THREE.NoColorSpace, 1);
}

// Layout -----------------------------------------------------------------------------------------

function textRects(text) {
  const rects = [];
  const range = document.createRange();
  const walker = document.createTreeWalker(text, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => (node.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT),
  });
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    range.selectNodeContents(node);
    for (const rect of range.getClientRects()) rects.push(rect);
  }
  for (const element of text.querySelectorAll('a, button, input, select, img, svg, .dw-command, .dw-badge')) rects.push(element.getBoundingClientRect());
  return rects.filter((rect) => rect.width > 0 && rect.height > 0);
}

// The largest box of the composition's shape clear of the text: beside it on a wide screen, or
// above it where the stylesheet leaves room on a phone. Returns its centre and height, relative
// to the art.
function placement(root) {
  const hero = root.closest('.dw-hero') || root.parentElement;
  const box = root.getBoundingClientRect();
  const text = hero.querySelector('.dw-hero__text');
  const strip = hero.querySelector('.dw-strip');
  const shellElement = hero.querySelector('.dw-hero__grid') || hero.querySelector('.dw-shell') || hero;
  const shellStyle = getComputedStyle(shellElement);
  const shellBox = shellElement.getBoundingClientRect();
  const shell = { left: shellBox.left + parseFloat(shellStyle.paddingLeft), right: shellBox.right - parseFloat(shellStyle.paddingRight) };
  const floor = strip ? strip.getBoundingClientRect().top : box.bottom - 24;
  const rects = text ? textRects(text) : [];
  const aspect = COMP.width / COMP.height;
  if (!rects.length) return { x: box.width * 0.7, y: box.height * 0.45, size: Math.min(box.height * 0.6, box.width * 0.4 / aspect), above: false };
  const right = Math.max(...rects.map((rect) => rect.right));
  const top = Math.min(...rects.map((rect) => rect.top));
  const candidates = [
    { x0: right + 24, x1: Math.min(box.right - 12, shell.right), y0: box.top + 14, y1: floor - 14, above: false },
    { x0: shell.left - 8, x1: shell.right + 8, y0: box.top + 6, y1: top - 10, above: true },
  ].map((region) => ({ ...region, size: Math.max(0, Math.min(region.y1 - region.y0, (region.x1 - region.x0) / aspect)) }));
  const best = candidates.reduce((a, b) => (b.size > a.size ? b : a));
  const size = Math.min(best.size * (best.above ? 0.97 : 0.92), 470);
  const x = best.above ? (best.x0 + best.x1) / 2 : Math.min(best.x1 - size * aspect / 2, (best.x0 + best.x1) / 2 + (best.x1 - best.x0 - size * aspect) * 0.3);
  return { x: x - box.left, y: (best.y0 + best.y1) / 2 - box.top, size, above: best.above };
}

// The scene --------------------------------------------------------------------------------------

function mount(root) {
  const hero = root.closest('.dw-hero') || root;
  const still = document.createElement('img');
  still.className = 'dini-hero__still';
  still.alt = '';
  still.decoding = 'async';
  still.src = new URL('../img/dream-ini-hero.webp', import.meta.url).href;
  root.append(still);

  function placeStill() {
    const spot = placement(root);
    const aspect = COMP.width / COMP.height;
    const size = spot.size * STILL_MARGIN;
    Object.assign(still.style, {
      left: `${spot.x - size * aspect / 2}px`,
      top: `${spot.y - size / 2}px`,
      width: `${size * aspect}px`,
      height: `${size}px`,
    });
    root.classList.add('is-placed');
  }

  const canvas = document.createElement('canvas');
  canvas.className = 'dini-hero__canvas';
  let renderer;
  try {
    renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: true, premultipliedAlpha: true, powerPreference: 'high-performance' });
  } catch {
    placeStill();
    return;
  }
  if (!renderer.capabilities.isWebGL2) {
    renderer.dispose();
    placeStill();
    return;
  }
  renderer.autoClear = false;
  renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
  renderer.setClearColor(0x000000, 0);
  root.append(canvas);

  const small = Math.min(innerWidth, innerHeight) < 700;
  const floatTargets = renderer.extensions.has('EXT_color_buffer_float') || renderer.extensions.has('EXT_color_buffer_half_float');
  const targetType = floatTargets ? THREE.HalfFloatType : THREE.UnsignedByteType;
  const makeTarget = () => new THREE.WebGLRenderTarget(1, 1, { type: targetType, depthBuffer: false });
  const sceneTarget = new THREE.WebGLRenderTarget(1, 1, { type: targetType, samples: 4 });
  const bloomTargets = [makeTarget(), makeTarget(), makeTarget(), makeTarget()];
  const anisotropy = Math.min(8, renderer.capabilities.getMaxAnisotropy());

  const accent = cssColor('--dw-accent', '#e0b867');
  const ink = cssColor('--dw-text', '#efe6d6');
  const mono = cssValue('--dw-font-mono', 'ui-monospace, monospace');

  const camera = new THREE.PerspectiveCamera(28, 1, 0.1, 80);
  const cameraBase = new THREE.Vector3(0, 0.25, 11);
  camera.position.copy(cameraBase);
  camera.lookAt(0, 0, 0);

  const scene = new THREE.Scene();
  const envTarget = environment(renderer, accent);
  scene.environment = envTarget.texture;

  // The rig holds the whole composition; the layout scales and places it.
  const rig = new THREE.Group();
  scene.add(rig);

  // The medallion.
  const traces = generateTraces();
  const board = bakeBoard(small ? 1024 : 2048, traces, mono, anisotropy);
  const flowUniforms = {
    uFlowTime: { value: 0 },
    uFlowGain: { value: 4.5 },
    uFlowBase: { value: 0.06 },
    uFlowColor: { value: accent.clone().lerp(new THREE.Color(1, 0.95, 0.85), 0.5).multiplyScalar(1.3) },
  };
  const faceMaterial = withFlow(new THREE.MeshPhysicalMaterial({
    map: board.map,
    normalMap: board.normal,
    normalScale: new THREE.Vector2(0.9, 0.9),
    aoMap: board.surface,
    roughnessMap: board.surface,
    roughness: 1,
    metalnessMap: board.surface,
    metalness: 1,
    clearcoat: 0.3,
    clearcoatRoughness: 0.3,
    emissive: new THREE.Color(1, 1, 1),
    envMapIntensity: 1.15,
  }), board.flow, flowUniforms);
  const medallion = new THREE.Group();
  rig.add(medallion);
  const face = new THREE.Mesh(new THREE.CircleGeometry(1, small ? 96 : 160), faceMaterial);
  face.position.z = FACE_Z;
  const edgeMaterial = new THREE.MeshPhysicalMaterial({ color: new THREE.Color(0.55, 0.4, 0.2), metalness: 1, roughness: 0.32, envMapIntensity: 1.2 });
  const edge = new THREE.Mesh(new THREE.CylinderGeometry(1, 1, FACE_Z * 2, small ? 96 : 160, 1, true), edgeMaterial);
  edge.rotation.x = Math.PI / 2;
  const back = new THREE.Mesh(new THREE.CircleGeometry(1, 64), new THREE.MeshStandardMaterial({ color: 0x0d0906, roughness: 0.6 }));
  back.position.z = -FACE_Z;
  back.rotation.y = Math.PI;
  const goldMaterial = new THREE.MeshPhysicalMaterial({ color: accent.clone().lerp(new THREE.Color(1, 0.85, 0.5), 0.4), metalness: 1, roughness: 0.18, clearcoat: 0.4, envMapIntensity: 1.4 });
  const ring = new THREE.Mesh(new THREE.TorusGeometry(0.16, 0.026, 18, 96), goldMaterial);
  ring.position.z = FACE_Z + 0.02;
  const lensUniforms = { uTime: { value: 0 }, uPulse: { value: 0 }, uActivity: { value: 0 }, uColor: { value: accent.clone().lerp(new THREE.Color(1, 0.9, 0.7), 0.3) } };
  const lens = new THREE.Mesh(new THREE.CircleGeometry(0.14, 64), new THREE.ShaderMaterial({ vertexShader: PLANE_VERTEX, fragmentShader: LENS_FRAGMENT, uniforms: lensUniforms, ...PREMULTIPLIED }));
  lens.position.z = FACE_Z + 0.012;
  medallion.add(face, edge, back, ring, lens);

  // The cord.
  const braid = new THREE.Group();
  const braidTexture = braidMaps(anisotropy);
  const braidMaterial = new THREE.MeshPhysicalMaterial({
    map: braidTexture.map,
    normalMap: braidTexture.normal,
    normalScale: new THREE.Vector2(0.9, 0.9),
    roughness: 0.5,
    metalness: 0.15,
    sheen: 0.7,
    sheenColor: new THREE.Color(1.0, 0.7, 0.45),
    sheenRoughness: 0.4,
    envMapIntensity: 0.9,
  });
  for (let i = 0; i < 3; i++) {
    const strand = new THREE.Mesh(new THREE.TubeGeometry(new Strand(1.08, 0.046, 26, i * Math.PI * 2 / 3), small ? 520 : 960, 0.041, small ? 8 : 12, true), braidMaterial);
    braid.add(strand);
  }
  medallion.add(braid);

  // The key halo: two rings of real fallback keys turning opposite ways just outside the cord.
  const keyrings = [];
  const ringKeys = [
    ['Weather_Clear_Cloud_Texture', 'Moons_Masser_Size', 'Water_Map_Alpha', 'LightAttenuation_UseConstant', 'FontColor_color_normal', 'Blood_Model_0', 'Level_Up_Level2', 'Map_Travel_Boat_Red'],
    ['Question_1_Question', 'Inventory_UniformScaling', 'Movies_Company_Logo', 'General_Werewolf_FOV', 'Weather_Thunderstorm_Thunder_Sound_ID_0', 'PixelWater_TileCount', 'Weather_Sunrise_Time', 'Moons_Secunda_Speed'],
  ];
  [[1.245, 0.02], [1.345, -0.014]].forEach(([radius, speed], i) => {
    const outer = 1.42;
    const keyTexture = keyringTexture(mono, ringKeys[i], radius / outer, small ? 1024 : 2048);
    const uniforms = { tKeys: { value: keyTexture }, uGain: { value: 0.35 }, uColor: { value: accent.clone().lerp(new THREE.Color(1, 0.95, 0.85), 0.3) } };
    const mesh = new THREE.Mesh(new THREE.RingGeometry(radius - 0.05, radius + 0.05, 256, 1), new THREE.ShaderMaterial({ vertexShader: PLANE_VERTEX, fragmentShader: KEYRING_FRAGMENT, uniforms, ...LIGHT }));
    // Planar uvs over the ring's own square, so the texture's circle lands on it.
    const uv = mesh.geometry.attributes.uv;
    const position = mesh.geometry.attributes.position;
    for (let v = 0; v < uv.count; v++) uv.setXY(v, position.getX(v) / outer * 0.5 + 0.5, position.getY(v) / outer * 0.5 + 0.5);
    mesh.position.z = -0.01;
    mesh.renderOrder = 1;
    medallion.add(mesh);
    keyrings.push({ mesh, uniforms, speed });
  });

  // The backlight.
  const haloUniforms = { uTime: { value: 0 }, uGain: { value: 0.5 }, uColor: { value: accent.clone().lerp(new THREE.Color(1, 0.8, 0.5), 0.3) } };
  const halo = new THREE.Mesh(new THREE.PlaneGeometry(4.2, 4.2), new THREE.ShaderMaterial({ vertexShader: PLANE_VERTEX, fragmentShader: HALO_FRAGMENT, uniforms: haloUniforms, ...LIGHT }));
  halo.position.z = -0.6;
  halo.renderOrder = -1;
  rig.add(halo);

  // The two pages and the plaque that names the current section.
  const sourcePage = createPage('source', mono, accent, ink, small);
  const ledgerPage = createPage('ledger', mono, accent, ink, small);
  const pageMesh = (page, spec) => {
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(spec.width, spec.height), new THREE.ShaderMaterial({
      vertexShader: PLANE_VERTEX,
      fragmentShader: PAGE_FRAGMENT,
      uniforms: { tPage: { value: page.texture }, uGlow: { value: 1.5 }, uOpacity: { value: 1 } },
      ...PREMULTIPLIED,
    }));
    mesh.position.set(spec.x, spec.y, spec.z);
    mesh.rotation.y = spec.turn;
    mesh.renderOrder = 1;
    rig.add(mesh);
    return mesh;
  };
  const sourceMesh = pageMesh(sourcePage, SOURCE);
  const ledgerMesh = pageMesh(ledgerPage, LEDGER);

  const atlas = createAtlas(mono);
  const cards = [];
  for (let row = 0; row < ATLAS.rows; row++) {
    const uniforms = {
      tAtlas: { value: atlas.texture },
      uRect: { value: new THREE.Vector4() },
      uPad: { value: new THREE.Vector2(0.05, 0.22) },
      uAspect: { value: 8 },
      uOpacity: { value: 0 },
      uDissolve: { value: 0 },
      uFlash: { value: 0 },
      uTint: { value: ink.clone() },
      uEdge: { value: accent.clone() },
    };
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), new THREE.ShaderMaterial({ vertexShader: PLANE_VERTEX, fragmentShader: CARD_FRAGMENT, uniforms, ...PREMULTIPLIED, side: THREE.DoubleSide }));
    mesh.visible = false;
    mesh.renderOrder = 3;
    rig.add(mesh);
    cards.push({ row, mesh, uniforms, active: false });
  }
  const plaque = cards.pop();
  plaque.mesh.renderOrder = 2;

  // Packets on the traces (in the medallion's frame) and sparks (in the rig's).
  const makePoints = (count, parent) => {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(new Float32Array(count * 3), 3));
    geometry.setAttribute('aSize', new THREE.BufferAttribute(new Float32Array(count), 1));
    geometry.setAttribute('aAlpha', new THREE.BufferAttribute(new Float32Array(count), 1));
    geometry.setAttribute('aColor', new THREE.BufferAttribute(new Float32Array(count * 3), 3));
    const uniforms = { uPixel: { value: 500 }, uScale: { value: 1 } };
    const points = new THREE.Points(geometry, new THREE.ShaderMaterial({ vertexShader: POINT_VERTEX, fragmentShader: POINT_FRAGMENT, uniforms, ...LIGHT }));
    points.frustumCulled = false;
    points.renderOrder = 4;
    parent.add(points);
    return { geometry, uniforms, points, items: Array.from({ length: count }, () => ({ active: false })) };
  };
  const packets = makePoints(small ? 260 : 520, medallion);
  const sparks = makePoints(small ? 120 : 220, rig);

  const motes = (() => {
    const count = small ? 40 : 80;
    const geometry = new THREE.BufferGeometry();
    const seeds = new Float32Array(count * 4);
    const next = random(1402);
    for (let i = 0; i < seeds.length; i++) seeds[i] = next();
    geometry.setAttribute('position', new THREE.BufferAttribute(new Float32Array(count * 3), 3));
    geometry.setAttribute('aSeed', new THREE.BufferAttribute(seeds, 4));
    const uniforms = {
      tGlyphs: { value: glyphTexture(mono) },
      uTime: { value: 0 },
      uPixel: { value: 500 },
      uScale: { value: 1 },
      uCenter: { value: new THREE.Vector3() },
      uBox: { value: new THREE.Vector2(COMP.width * 1.1, COMP.height * 1.3) },
      uColor: { value: accent.clone().multiplyScalar(0.9) },
    };
    const points = new THREE.Points(geometry, new THREE.ShaderMaterial({ vertexShader: MOTE_VERTEX, fragmentShader: MOTE_FRAGMENT, uniforms, ...LIGHT }));
    points.frustumCulled = false;
    points.renderOrder = 0;
    scene.add(points);
    return { uniforms };
  })();

  const waveUniforms = { uAge: { value: 1 }, uColor: { value: accent.clone().lerp(new THREE.Color(1, 0.95, 0.85), 0.5) } };
  const wave = new THREE.Mesh(new THREE.PlaneGeometry(6, 6), new THREE.ShaderMaterial({ vertexShader: PLANE_VERTEX, fragmentShader: WAVE_FRAGMENT, uniforms: waveUniforms, ...LIGHT }));
  wave.position.z = 0.2;
  wave.renderOrder = 5;
  wave.visible = false;
  rig.add(wave);

  // Lights: a warm key, cool and accent rims, and a lamp that follows the pointer.
  const key = new THREE.DirectionalLight(new THREE.Color(1.0, 0.88, 0.7), 1.6);
  key.position.set(-4, 5, 6);
  const rim = new THREE.DirectionalLight(new THREE.Color(0.7, 0.8, 1.0), 1.8);
  rim.position.set(5, 2, -5);
  const backLight = new THREE.DirectionalLight(accent, 1.6);
  backLight.position.set(-5, -2, -4);
  const lamp = new THREE.PointLight(new THREE.Color(1.0, 0.9, 0.75), 0, 0, 2);
  scene.add(key, rim, backLight, lamp);

  // Post-processing.
  const quad = new THREE.PlaneGeometry(2, 2);
  const postScene = new THREE.Scene();
  const postCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  const postQuad = new THREE.Mesh(quad);
  postQuad.frustumCulled = false;
  postScene.add(postQuad);
  const brightMaterial = fullscreenMaterial(BRIGHT_FRAGMENT, { tInput: { value: sceneTarget.texture }, uThreshold: { value: 0.85 } });
  const blurMaterial = fullscreenMaterial(BLUR_FRAGMENT, { tInput: { value: null }, uDirection: { value: new THREE.Vector2() } });
  const copyMaterial = fullscreenMaterial(/* glsl */ `
    uniform sampler2D tInput;
    varying vec2 vUv;
    void main() { gl_FragColor = texture2D(tInput, vUv); }
  `, { tInput: { value: null } });
  const compositeMaterial = fullscreenMaterial(COMPOSITE_FRAGMENT, {
    tScene: { value: sceneTarget.texture },
    tBloomNear: { value: bloomTargets[0].texture },
    tBloomFar: { value: bloomTargets[2].texture },
    uTime: { value: 0 },
    uExposure: { value: 0.95 },
  });
  compositeMaterial.transparent = true;
  compositeMaterial.blending = THREE.NoBlending;
  function pass(material, target) {
    postQuad.material = material;
    renderer.setRenderTarget(target);
    renderer.render(postScene, postCamera);
  }
  function blur(target, scratch, radius) {
    blurMaterial.uniforms.tInput.value = target.texture;
    blurMaterial.uniforms.uDirection.value.set(radius / target.width, 0);
    pass(blurMaterial, scratch);
    blurMaterial.uniforms.tInput.value = scratch.texture;
    blurMaterial.uniforms.uDirection.value.set(0, radius / target.height);
    pass(blurMaterial, target);
  }

  // Layout.
  const quality = { level: 1, slow: 0 };
  let width = 1;
  let height = 1;
  let scale = 1;
  let place = { x: 0, y: 0, size: 0, above: false };
  const anchor = new THREE.Vector3();
  const raycaster = new THREE.Raycaster();
  const plane = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  const ndc = new THREE.Vector2();

  function layout() {
    const rect = root.getBoundingClientRect();
    width = Math.max(1, Math.round(rect.width));
    height = Math.max(1, Math.round(rect.height));
    const dpr = Math.min(window.devicePixelRatio || 1, small ? 1.5 : 1.75) * quality.level;
    renderer.setPixelRatio(dpr);
    renderer.setSize(width, height, false);
    const w = Math.max(1, Math.floor(width * dpr));
    const h = Math.max(1, Math.floor(height * dpr));
    sceneTarget.setSize(w, h);
    bloomTargets[0].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[1].setSize(Math.max(1, w >> 2), Math.max(1, h >> 2));
    bloomTargets[2].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    bloomTargets[3].setSize(Math.max(1, w >> 3), Math.max(1, h >> 3));
    camera.aspect = width / height;
    camera.position.copy(cameraBase);
    camera.lookAt(0, 0, 0);
    camera.updateProjectionMatrix();
    camera.updateMatrixWorld();

    place = placement(root);
    placeStill();
    ndc.set(place.x / width * 2 - 1, -(place.y / height * 2 - 1));
    raycaster.setFromCamera(ndc, camera);
    plane.constant = 0;
    raycaster.ray.intersectPlane(plane, anchor);
    const unitsPerPixel = 2 * camera.position.distanceTo(anchor) * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) / height;
    scale = Math.max(0.02, place.size * unitsPerPixel / COMP.height);
    rig.scale.setScalar(scale);
    rig.position.set(anchor.x - COMP.x * scale, anchor.y - COMP.y * scale, 0);
    const pixel = h / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)));
    packets.uniforms.uPixel.value = pixel;
    packets.uniforms.uScale.value = scale;
    sparks.uniforms.uPixel.value = pixel;
    sparks.uniforms.uScale.value = scale;
    motes.uniforms.uPixel.value = pixel;
    motes.uniforms.uScale.value = scale;
    motes.uniforms.uCenter.value.copy(anchor);
  }

  // The importer ---------------------------------------------------------------------------------

  const state = { mode: 'import', selections: 0, torrents: 0, rewrites: 0, dropped: 0, last: '' };
  window.__diniState = state;
  const tmp = new THREE.Vector3();
  const tmp2 = new THREE.Vector3();
  const flat = { x: 0, y: 0 };

  // The INI page: the script's lines, each with its state; the cfg: kept lines, then written.
  let sourceRows = [];
  let sourceHead = -1;
  let sourceFirst = 0;
  let ledgerRows = [];
  let ledgerBadge = '';
  let pagesDirty = true;

  function resetPages() {
    sourceRows = SCRIPT.map((entry) => ({ text: entry.ini, style: entry.header ? 'header' : entry.drop && entry.ini.startsWith(';') ? 'dim' : 'plain', glow: 0 }));
    sourceHead = -1;
    sourceFirst = 0;
    ledgerRows = KEPT.map((text) => ({ text, style: 'dim', glow: 0 }));
    ledgerBadge = '';
    pagesDirty = true;
  }

  function appendLedger(text) {
    ledgerRows.push({ text, style: 'plain', glow: 1 });
    if (ledgerRows.length > 400) ledgerRows.splice(KEPT.length, ledgerRows.length - 400);
    pagesDirty = true;
  }

  function ledgerFirst() {
    return Math.max(0, ledgerRows.length - ledgerPage.visible);
  }

  function drawPages() {
    const sourceTop = Math.max(0, Math.min(sourceHead - Math.floor(sourcePage.visible * 0.45), sourceRows.length - sourcePage.visible));
    sourceFirst += (sourceTop - sourceFirst) * 0.35;
    sourcePage.draw(sourceRows, Math.max(0, Math.round(sourceFirst)), '');
    ledgerPage.draw(ledgerRows, ledgerFirst(), ledgerBadge);
    pagesDirty = false;
  }

  function pageRow(page, mesh, spec, row, first, target) {
    page.rowPosition(row, first, spec.width, spec.height, target);
    mesh.updateMatrix();
    return target.applyMatrix4(mesh.matrix);
  }

  function padInRig(trace, target) {
    target.set(trace.pad[0], trace.pad[1], FACE_Z + 0.03);
    medallion.updateMatrix();
    return target.applyMatrix4(medallion.matrix);
  }

  function pickTrace(input) {
    const pool = traces.filter((trace) => trace.input === input);
    pool.sort((a, b) => a.busy - b.busy || Math.random() - 0.5);
    const trace = pool[0];
    trace.busy += 1;
    return trace;
  }

  // Cards fly on a curve that bows towards the viewer.
  function flyCard(text, from, to, options) {
    const card = cards.find((item) => !item.active);
    if (!card) {
      options.arrive?.();
      return null;
    }
    const layout = atlas.write(card.row, text);
    card.active = true;
    card.text = text;
    card.age = 0;
    card.duration = options.duration;
    card.from = from.clone();
    card.to = to;
    card.lift = options.lift ?? 0.55;
    card.startScale = options.startScale ?? 0.55;
    card.endScale = options.endScale ?? 0.5;
    card.drop = options.drop ?? -1;
    card.arrive = options.arrive;
    card.burnt = false;
    card.uniforms.uRect.value.copy(layout.rect);
    const padX = 0.22 / (layout.aspect + 0.44);
    card.uniforms.uPad.value.set(padX, 0.2);
    card.aspect = layout.aspect + 0.44;
    card.uniforms.uAspect.value = card.aspect;
    card.uniforms.uTint.value.copy(options.tint || ink);
    card.uniforms.uDissolve.value = 0;
    card.uniforms.uFlash.value = 0;
    card.uniforms.uOpacity.value = 0;
    card.mesh.visible = true;
    return card;
  }

  function burst(position, count, color, spread) {
    for (let i = 0; i < count; i++) {
      const spark = sparks.items.find((item) => !item.active);
      if (!spark) return;
      spark.active = true;
      spark.position = position.clone();
      spark.velocity = new THREE.Vector3((Math.random() - 0.5) * spread, (Math.random() - 0.2) * spread, (Math.random() - 0.5) * spread * 0.5);
      spark.life = 0;
      spark.span = 0.6 + Math.random() * 0.8;
      spark.color = color;
      spark.size = 0.012 + Math.random() * 0.02;
    }
  }

  function sendPacket(trace, inward, speed, arrive, size = 0.085) {
    const packet = packets.items.find((item) => !item.active);
    if (!packet) {
      arrive?.();
      return;
    }
    packet.active = true;
    packet.trace = trace;
    packet.inward = inward;
    packet.distance = inward ? trace.length : 0;
    packet.speed = speed;
    packet.arrive = arrive;
    packet.size = size;
  }

  let pulse = 0;
  let activity = 0;
  let plaqueText = '';
  function setPlaque(text) {
    plaqueText = text;
    const layout = atlas.write(plaque.row, text);
    plaque.uniforms.uRect.value.copy(layout.rect);
    plaque.aspect = layout.aspect + 0.44;
    plaque.uniforms.uPad.value.set(0.22 / plaque.aspect, 0.2);
    plaque.uniforms.uAspect.value = plaque.aspect;
    plaque.uniforms.uTint.value.copy(accent).lerp(new THREE.Color(1, 0.95, 0.85), 0.5);
    plaque.uniforms.uFlash.value = 1;
    plaque.mesh.visible = !!text;
    plaque.mesh.scale.set(CARD_HEIGHT * 1.05 * plaque.aspect, CARD_HEIGHT * 1.05, 1);
  }

  // The regular import: one line of the script at a time.
  let scriptIndex = 0;
  let nextEntry = 1.2;
  let cycleRestart = -1;

  function emitOutputs(lines) {
    lines.forEach((text, i) => {
      schedule(i * 0.28, () => {
        const trace = pickTrace(false);
        sendPacket(trace, false, 1.5, () => {
          trace.busy -= 1;
          const from = padInRig(trace, new THREE.Vector3());
          flyCard(text, from, () => pageRow(ledgerPage, ledgerMesh, LEDGER, Math.min(ledgerRows.length, ledgerFirst() + ledgerPage.visible - 1), ledgerFirst(), tmp2), {
            duration: 1.05,
            lift: 0.5,
            startScale: 0.35,
            endScale: 0.42,
            tint: accent.clone().lerp(new THREE.Color(1, 0.96, 0.88), 0.55),
            arrive: () => appendLedger(text),
          });
        });
      });
    });
  }

  function parseAtRing(entry) {
    pulse = 1;
    if (entry.out) emitOutputs(entry.out);
  }

  function runEntry(index) {
    const entry = SCRIPT[index];
    sourceHead = index;
    sourceRows.forEach((row, i) => { row.glow = i === index ? 1 : 0; });
    pagesDirty = true;
    const from = pageRow(sourcePage, sourceMesh, SOURCE, index, Math.round(sourceFirst), new THREE.Vector3());
    if (entry.header) {
      flyCard(entry.ini, from, () => tmp2.set(PLAQUE.x, PLAQUE.y, PLAQUE.z), {
        duration: 0.8,
        lift: 0.35,
        startScale: 0.4,
        endScale: 1.0,
        tint: accent,
        arrive: () => setPlaque(entry.ini),
      });
      return 0.85;
    }
    if (entry.drop) {
      const trace = pickTrace(true);
      trace.busy -= 1;
      flyCard(entry.ini, from, () => padInRig(trace, tmp2), {
        duration: 1.1,
        startScale: 0.4,
        endScale: 0.55,
        drop: 0.62,
        tint: new THREE.Color(0.75, 0.68, 0.58),
        arrive: () => { sourceRows[index].style = 'struck'; pagesDirty = true; },
      });
      return 1.35;
    }
    const trace = pickTrace(true);
    flyCard(entry.ini, from, () => padInRig(trace, tmp2), {
      duration: 1.0,
      startScale: 0.4,
      endScale: 0.3,
      arrive: () => {
        sendPacket(trace, true, 1.4, () => {
          trace.busy -= 1;
          parseAtRing(entry);
        });
      },
    });
    return 1.25 + (entry.out ? entry.out.length - 1 : 0) * 0.3;
  }

  // Deferred calls on the scene's own clock, so a still frame can be simulated.
  let clockTime = 0;
  const timers = [];
  function schedule(delay, run) {
    timers.push({ at: clockTime + delay, run });
  }

  function startCycle() {
    resetPages();
    setPlaque('');
    scriptIndex = 0;
    nextEntry = clockTime + 0.9;
    // encoding= is always written, from the options or the cfg, before any line is read.
    schedule(0.3, () => emitOutputs([ENCODING]));
  }

  // The whole file: every key the crate knows, as fast as the eye allows.
  let torrent = null;
  function startTorrent() {
    state.torrents += 1;
    state.mode = 'torrent';
    const keys = fallbackKeys();
    for (const card of cards) {
      if (card.active) card.uniforms.uDissolve.value = Math.max(card.uniforms.uDissolve.value, 0.01);
    }
    timers.length = 0;
    resetPages();
    ledgerRows.push({ text: ENCODING, style: 'plain', glow: 0 });
    sourceRows = [];
    let section = '';
    for (const [name, key] of keys) {
      if (name !== section) {
        section = name;
        sourceRows.push({ text: `[${name}]`, style: 'header', glow: 0 });
      }
      sourceRows.push({ text: `${key}=${sampleValue(name, key)}`, style: 'plain', glow: 0, name, key });
    }
    torrent = { age: 0, read: 0, written: 0, emitted: 0, rows: sourceRows, section: '' };
  }

  // Illustrative values of the right kind: colours as R,G,B, textures and models by name, numbers
  // elsewhere. A real Morrowind.ini's own values pass through unchanged.
  function sampleValue(section, key) {
    const n = (salt) => (key.length * 37 + section.length * 11 + salt * 53) % 256;
    if (section === 'General') return '150';
    if (section === 'FontColor' || /Color/.test(key)) return `${n(1)},${n(2)},${n(3)}`;
    if (/Texture/.test(key)) return section === 'Blood' ? 'Tx_Blood.tga' : `Tx_Sky_${section.replace('Weather ', '') || 'Clear'}.dds`;
    if (/^Model/.test(key)) return 'BloodSplat.nif';
    if (section === 'Movies') return `${key.toLowerCase().replace(/ /g, '_')}.bik`;
    if (/Sound ID/.test(key)) return `Thunder${key.slice(-1)}`;
    if (/Question|Level Up/.test(section)) return '\u2026';
    if (/Red|Green|Blue|R$|G$|B$/.test(key)) return String(n(4));
    return (((key.length * 7) % 19) / 10 + 0.1).toFixed(1);
  }

  function updateTorrent(dt) {
    const t = torrent;
    t.age += dt;
    const windup = 0.7;
    const flood = 3.3;
    const rows = t.rows;
    if (t.age > windup && t.age < windup + flood) {
      const progress = (t.age - windup) / flood;
      const target = Math.floor(progress * rows.length);
      while (t.read < target) {
        const row = rows[t.read];
        sourceHead = t.read;
        if (row.style === 'header') {
          t.section = row.text;
        } else if (row.name === 'Fonts') {
          row.style = 'struck';
        } else {
          const written = `fallback=${`${row.name}:${row.key}`.replace(/[ :]/g, '_')},${row.text.slice(row.text.indexOf('=') + 1)}`;
          ledgerRows.push({ text: written, style: 'plain', glow: 0.6 });
          t.written += 1;
        }
        t.read += 1;
      }
      if (t.section !== plaqueText) setPlaque(t.section);
      if (ledgerRows.length > 400) ledgerRows.splice(KEPT.length + 1, ledgerRows.length - 400);
      ledgerBadge = `${t.written} fallback=`;
      pagesDirty = true;
      // A flood of packets in and out.
      const rate = small ? 90 : 170;
      t.emitted += dt * rate;
      while (t.emitted > 1) {
        t.emitted -= 1;
        const trace = traces[(Math.random() * traces.length) | 0];
        if (trace.input) {
          sendPacket(trace, true, 3.2 + Math.random() * 1.5, () => {
            const out = traces.filter((item) => !item.input);
            sendPacket(out[(Math.random() * out.length) | 0], false, 3.2 + Math.random() * 1.5, null, 0.04);
          }, 0.04);
        } else {
          sendPacket(trace, false, 3.2 + Math.random() * 1.5, null, 0.04);
        }
      }
    }
    if (t.age >= windup + flood && !t.finished) {
      t.finished = true;
      while (t.read < rows.length) {
        const row = rows[t.read++];
        if (row.style !== 'header' && row.name !== 'Fonts') {
          ledgerRows.push({ text: `fallback=${`${row.name}:${row.key}`.replace(/[ :]/g, '_')},${row.text.slice(row.text.indexOf('=') + 1)}`, style: 'plain', glow: 1 });
          t.written += 1;
        } else if (row.name === 'Fonts') {
          row.style = 'struck';
        }
      }
      ledgerBadge = `${t.written} fallback=`;
      pagesDirty = true;
      wave.visible = true;
      waveUniforms.uAge.value = 0;
      pulse = 1;
    }
    if (t.age > windup + flood + 3.5) {
      torrent = null;
      state.mode = 'import';
      startCycle();
    }
  }

  // The cfg rewritten from the top: the whole of it glows as it is written again.
  let rewrite = null;
  function startRewrite() {
    state.rewrites += 1;
    rewrite = { age: 0 };
    for (const row of ledgerRows) row.glow = 0;
    const out = traces.filter((trace) => !trace.input);
    for (let i = 0; i < 28; i++) schedule(i * 0.05, () => sendPacket(out[i % out.length], false, 2.2, null, 0.05));
    pulse = 1;
    ledgerMesh.updateMatrix();
    burst(tmp.set(0, LEDGER.height * 0.35, 0.05).applyMatrix4(ledgerMesh.matrix).clone(), small ? 24 : 48, accent.clone().lerp(new THREE.Color(1, 0.95, 0.85), 0.5), 1.2);
  }

  function updateRewrite(dt) {
    rewrite.age += dt;
    const first = ledgerFirst();
    const lit = Math.floor(rewrite.age / 1.6 * ledgerPage.visible);
    ledgerRows.forEach((row, i) => {
      const position = i - first;
      row.glow = position <= lit && position >= lit - 2 ? 1 : row.glow * 0.92;
    });
    pagesDirty = true;
    ledgerMesh.material.uniforms.uGlow.value = 1.5 + Math.sin(Math.PI * Math.min(1, rewrite.age / 2.2)) * 3;
    if (rewrite.age > 2.2) {
      ledgerMesh.material.uniforms.uGlow.value = 1.5;
      for (const row of ledgerRows) row.glow = 0;
      rewrite = null;
    }
  }

  // A selection in the hero's title or summary, parsed as an INI line by the crate's rules.
  function parseSelection(text) {
    state.selections += 1;
    state.last = text;
    if (reduceMotion) return;
    if (/morrowind\.ini/i.test(text)) {
      if (!torrent) startTorrent();
      return;
    }
    if (/openmw\.cfg/i.test(text)) {
      if (!rewrite && !torrent) startRewrite();
      return;
    }
    const line = text.length > 42 ? `${text.slice(0, 41)}\u2026` : text;
    const header = /^\[[^\]]+\]$/.test(text);
    const pair = text.indexOf('=') > 0;
    const trace = pickTrace(true);
    const from = tmp.set(SOURCE.x + 0.2, SOURCE.y + 0.95, SOURCE.z + 0.4).clone();
    if (header) {
      trace.busy -= 1;
      flyCard(text, from, () => tmp2.set(PLAQUE.x, PLAQUE.y, PLAQUE.z), { duration: 0.9, startScale: 0.4, endScale: 1, tint: accent, arrive: () => setPlaque(text) });
      return;
    }
    if (pair) {
      const key = text.slice(0, text.indexOf('=')).trim();
      const value = text.slice(text.indexOf('=') + 1);
      const section = plaqueText ? plaqueText.slice(1, -1) : 'General';
      flyCard(line, from, () => padInRig(trace, tmp2), {
        duration: 1.0,
        startScale: 0.45,
        endScale: 0.3,
        arrive: () => sendPacket(trace, true, 1.4, () => {
          trace.busy -= 1;
          pulse = 1;
          emitOutputs([`fallback=${`${section}:${key}`.replace(/[ :]/g, '_')},${value}`]);
        }),
      });
      return;
    }
    // No = : the parser ignores it.
    state.dropped += 1;
    trace.busy -= 1;
    flyCard(line, from, () => padInRig(trace, tmp2), { duration: 1.2, startScale: 0.5, endScale: 0.6, drop: 0.6, tint: new THREE.Color(0.9, 0.82, 0.7) });
  }

  const scope = [hero.querySelector('.dw-hero__title'), hero.querySelector('.dw-hero__summary')].filter(Boolean);
  let selectionTimer = 0;
  let lastSelection = '';
  function checkSelection() {
    const selection = document.getSelection();
    if (!selection || selection.isCollapsed || !selection.rangeCount) {
      lastSelection = '';
      return;
    }
    const range = selection.getRangeAt(0);
    const inside = (node) => scope.some((element) => element.contains(node));
    if (!inside(range.startContainer) || !inside(range.endContainer)) return;
    const text = selection.toString().replace(/\s+/g, ' ').trim();
    if (!text || text === lastSelection) return;
    lastSelection = text;
    parseSelection(text);
  }
  document.addEventListener('selectionchange', () => {
    clearTimeout(selectionTimer);
    selectionTimer = setTimeout(checkSelection, 420);
  });

  // The pointer: a lamp in front of the board, which leans towards it.
  const pointer = new THREE.Vector2();
  let pointerActive = false;
  let lastPointer = 0;
  function onPointer(event) {
    const rect = root.getBoundingClientRect();
    pointer.set((event.clientX - rect.left) / rect.width * 2 - 1, -((event.clientY - rect.top) / rect.height * 2 - 1));
    pointerActive = true;
    lastPointer = performance.now();
  }
  if (!reduceMotion) {
    hero.addEventListener('pointermove', onPointer, { passive: true });
    hero.addEventListener('pointerleave', () => { pointerActive = false; }, { passive: true });
  }

  // The simulation ------------------------------------------------------------------------------

  const lean = new THREE.Vector2();
  const lampTarget = new THREE.Vector3();
  const lampPosition = new THREE.Vector3();
  let presence = 0;
  let braidSpin = 0;
  let boardSpin = 0;
  let flowClock = 0;

  function bezier(a, b, c, d, t, target) {
    const u = 1 - t;
    return target.set(
      u * u * u * a.x + 3 * u * u * t * b.x + 3 * u * t * t * c.x + t * t * t * d.x,
      u * u * u * a.y + 3 * u * u * t * b.y + 3 * u * t * t * c.y + t * t * t * d.y,
      u * u * u * a.z + 3 * u * u * t * b.z + 3 * u * t * t * c.z + t * t * t * d.z,
    );
  }
  const control1 = new THREE.Vector3();
  const control2 = new THREE.Vector3();
  const cardTarget = new THREE.Vector3();

  function updateCards(dt) {
    for (const card of cards) {
      if (!card.active) continue;
      card.age += dt;
      const t = Math.min(1, card.age / card.duration);
      const eased = t * t * (3 - 2 * t);
      cardTarget.copy(card.to());
      control1.copy(card.from).lerp(cardTarget, 0.3).add(tmp.set(0, card.lift * 0.6, card.lift));
      control2.copy(card.from).lerp(cardTarget, 0.75).add(tmp.set(0, card.lift * 0.3, card.lift * 0.8));
      bezier(card.from, control1, control2, cardTarget, eased, card.mesh.position);
      const grow = Math.sin(Math.PI * t);
      const size = CARD_HEIGHT * (card.startScale + (card.endScale - card.startScale) * eased + grow * 0.45);
      card.mesh.scale.set(size * card.aspect, size, 1);
      card.mesh.quaternion.copy(camera.quaternion);
      card.uniforms.uOpacity.value = Math.min(1, t * 6) * (card.drop < 0 ? 1 - Math.max(0, (t - 0.9) / 0.1) : 1);
      card.uniforms.uFlash.value = Math.max(0, 1 - t * 4);
      if (card.drop >= 0 && t > card.drop) {
        const burn = (t - card.drop) / (1 - card.drop);
        if (!card.burnt) {
          card.burnt = true;
          burst(card.mesh.position, small ? 18 : 34, new THREE.Color(1.0, 0.6, 0.25), 0.9);
        }
        card.uniforms.uDissolve.value = Math.min(1, burn * 1.1);
      } else if (card.uniforms.uDissolve.value > 0) {
        card.uniforms.uDissolve.value = Math.min(1, card.uniforms.uDissolve.value + dt * 2.5);
      }
      if (t >= 1 || card.uniforms.uDissolve.value >= 1) {
        card.active = false;
        card.mesh.visible = false;
        if (t >= 1 && card.uniforms.uDissolve.value < 0.5 && card.drop < 0) card.arrive?.();
        else if (card.drop >= 0) card.arrive?.();
      }
    }
    plaque.uniforms.uOpacity.value = plaqueText ? 1 : 0;
    plaque.uniforms.uFlash.value *= Math.exp(-dt * 3);
    plaque.mesh.position.set(PLAQUE.x, PLAQUE.y, PLAQUE.z);
    plaque.mesh.quaternion.copy(camera.quaternion);
  }

  function updatePoints(set, dt, place) {
    const position = set.geometry.attributes.position.array;
    const size = set.geometry.attributes.aSize.array;
    const alpha = set.geometry.attributes.aAlpha.array;
    const color = set.geometry.attributes.aColor.array;
    set.items.forEach((item, i) => {
      if (!item.active) {
        alpha[i] = 0;
        return;
      }
      place(item, dt, i, position, size, alpha, color);
    });
    set.geometry.attributes.position.needsUpdate = true;
    set.geometry.attributes.aSize.needsUpdate = true;
    set.geometry.attributes.aAlpha.needsUpdate = true;
    set.geometry.attributes.aColor.needsUpdate = true;
  }

  const packetColor = accent.clone().lerp(new THREE.Color(1, 0.97, 0.9), 0.6).multiplyScalar(2.6);
  function simulate(dt) {
    clockTime += dt;
    for (let i = timers.length - 1; i >= 0; i--) {
      if (timers[i].at <= clockTime) {
        const timer = timers.splice(i, 1)[0];
        timer.run();
      }
    }
    if (torrent) {
      updateTorrent(dt);
    } else {
      if (scriptIndex < SCRIPT.length && clockTime >= nextEntry) {
        nextEntry = clockTime + runEntry(scriptIndex);
        scriptIndex += 1;
        if (scriptIndex >= SCRIPT.length) cycleRestart = clockTime + 5.5;
      }
      if (cycleRestart > 0 && clockTime >= cycleRestart) {
        cycleRestart = -1;
        startCycle();
      }
    }
    if (rewrite) updateRewrite(dt);
    for (const row of ledgerRows) if (row.glow > 0 && !rewrite) row.glow = Math.max(0, row.glow - dt * 0.8);

    const surge = torrent ? Math.min(1, torrent.age / 0.7) * (torrent.finished ? Math.max(0, 1 - (torrent.age - 4.0) / 2.5) : 1) : 0;
    activity += ((surge > 0 ? 1.4 * surge : 0.15) - activity) * Math.min(1, dt * 3);
    pulse *= Math.exp(-dt * 2.4);
    flowClock += dt * (0.55 + surge * 2.2);
    braidSpin += dt * (0.03 + surge * 0.9);
    const spinTarget = torrent && !torrent.finished && torrent.age > 0.7 ? Math.PI * 2 * Math.min(1, (torrent.age - 0.7) / 3.3) : 0;
    boardSpin = torrent ? spinTarget : boardSpin * Math.exp(-dt * 2);
    if (waveUniforms.uAge.value < 1) waveUniforms.uAge.value = Math.min(1, waveUniforms.uAge.value + dt / 1.6);
    wave.visible = waveUniforms.uAge.value < 1;

    updateCards(dt);
    updatePoints(packets, dt, (item, step, i, position, size, alpha, color) => {
      item.distance += (item.inward ? -1 : 1) * item.speed * step;
      const done = item.inward ? item.distance <= 0 : item.distance >= item.trace.length;
      pointOnTrace(item.trace, item.distance, flat);
      position[i * 3] = flat.x;
      position[i * 3 + 1] = flat.y;
      position[i * 3 + 2] = FACE_Z + 0.02;
      size[i] = item.size;
      alpha[i] = 1;
      color[i * 3] = packetColor.r;
      color[i * 3 + 1] = packetColor.g;
      color[i * 3 + 2] = packetColor.b;
      if (done) {
        item.active = false;
        alpha[i] = 0;
        item.arrive?.();
      }
    });
    updatePoints(sparks, dt, (item, step, i, position, size, alpha, color) => {
      item.life += step;
      item.velocity.y -= step * 1.6;
      item.velocity.multiplyScalar(Math.exp(-step * 1.2));
      item.position.addScaledVector(item.velocity, step);
      const left = 1 - item.life / item.span;
      position[i * 3] = item.position.x;
      position[i * 3 + 1] = item.position.y;
      position[i * 3 + 2] = item.position.z;
      size[i] = item.size;
      alpha[i] = Math.max(0, left) * 2.5;
      color[i * 3] = item.color.r;
      color[i * 3 + 1] = item.color.g;
      color[i * 3 + 2] = item.color.b;
      if (left <= 0) {
        item.active = false;
        alpha[i] = 0;
      }
    });
  }

  function pose(dt) {
    // The lamp: the pointer while it moves over the hero, a slow orbit otherwise.
    const idle = !pointerActive || performance.now() - lastPointer > 4000;
    if (idle) {
      lampTarget.set(anchor.x + Math.sin(clockTime * 0.4) * 1.4 * scale, anchor.y + Math.cos(clockTime * 0.31) * 0.9 * scale, 2.4 * scale);
    } else {
      raycaster.setFromCamera(pointer, camera);
      plane.constant = -1.3 * scale;
      if (raycaster.ray.intersectPlane(plane, tmp)) lampTarget.copy(tmp);
      plane.constant = 0;
    }
    presence += ((idle ? 0.35 : 1) - presence) * Math.min(1, dt * 3);
    lampPosition.lerp(lampTarget, Math.min(1, dt * 6));
    lamp.position.copy(lampPosition);
    lamp.intensity = presence * 4.5 * scale * scale;

    const dx = (lampPosition.x - anchor.x) / scale;
    const dy = (lampPosition.y - anchor.y) / scale;
    const follow = Math.min(1, dt * 2.5);
    lean.x += (THREE.MathUtils.clamp(-dy * 0.06, -0.12, 0.12) - lean.x) * follow;
    lean.y += (THREE.MathUtils.clamp(dx * 0.06, -0.12, 0.12) - lean.y) * follow;
    medallion.rotation.set(lean.x + Math.sin(clockTime * 0.31) * 0.03, -0.16 + lean.y + Math.sin(clockTime * 0.23) * 0.05, boardSpin);
    medallion.position.set(0, Math.sin(clockTime * 0.7) * 0.02, 0);
    braid.rotation.z = braidSpin - boardSpin;
    for (const ring of keyrings) {
      ring.mesh.rotation.z += dt * ring.speed * (1 + activity * 25) - (ring.lastSpin === undefined ? 0 : boardSpin - ring.lastSpin);
      ring.lastSpin = boardSpin;
      ring.uniforms.uGain.value = 0.3 + activity * 0.9 + pulse * 0.25;
    }
    haloUniforms.uTime.value = clockTime;
    haloUniforms.uGain.value = 0.2 + pulse * 0.4 + activity * 1.4;
    braidTexture.map.offset.x = braidSpin * 3;
    braidTexture.normal.offset.x = braidSpin * 3;
    sourceMesh.position.y = SOURCE.y + Math.sin(clockTime * 0.5 + 1) * 0.02;
    ledgerMesh.position.y = LEDGER.y + Math.sin(clockTime * 0.45) * 0.02;
    camera.position.set(cameraBase.x + (idle ? 0 : pointer.x * 0.25), cameraBase.y + (idle ? 0 : pointer.y * 0.15), cameraBase.z);
    camera.lookAt(0, 0, 0);
    camera.updateMatrixWorld();

    flowUniforms.uFlowTime.value = flowClock;
    flowUniforms.uFlowGain.value = 4.5 + activity * 6.0;
    lensUniforms.uTime.value = clockTime;
    lensUniforms.uPulse.value = pulse;
    lensUniforms.uActivity.value = activity;
    motes.uniforms.uTime.value = clockTime;
    compositeMaterial.uniforms.uTime.value = clockTime;
  }

  let lastPages = -1;
  function render() {
    const settling = Math.abs(sourceFirst - Math.round(sourceFirst)) > 0.01;
    if ((pagesDirty || settling) && (clockTime - lastPages > 1 / 30 || reduceMotion || stillCapture)) {
      drawPages();
      lastPages = clockTime;
    }
    scene.updateMatrixWorld();
    renderer.setRenderTarget(sceneTarget);
    renderer.setClearColor(0x000000, 0);
    renderer.clear();
    renderer.render(scene, camera);
    pass(brightMaterial, bloomTargets[0]);
    blur(bloomTargets[0], bloomTargets[1], 1.0);
    blur(bloomTargets[0], bloomTargets[1], 2.0);
    copyMaterial.uniforms.tInput.value = bloomTargets[0].texture;
    pass(copyMaterial, bloomTargets[2]);
    blur(bloomTargets[2], bloomTargets[3], 1.5);
    blur(bloomTargets[2], bloomTargets[3], 3.0);
    renderer.setRenderTarget(null);
    renderer.setClearColor(0x000000, 0);
    renderer.clear();
    pass(compositeMaterial, null);
  }

  // The loop -------------------------------------------------------------------------------------

  const clock = new THREE.Clock();
  let visible = false;
  let running = false;
  let first = true;
  let lost = false;

  startCycle();
  layout();
  if (reduceMotion || stillCapture) {
    // A settled moment: the import under way, a few lines already written.
    for (let i = 0; i < 7.4 * 30; i++) simulate(1 / 30);
    pose(1);
  }

  function frame() {
    running = false;
    if (lost) return;
    const rawDt = clock.getDelta();
    const dt = Math.min(rawDt, 0.05);
    if (!reduceMotion && !stillCapture && rawDt < 0.5) {
      quality.slow = rawDt > 1 / 40 ? quality.slow + rawDt : Math.max(0, quality.slow - rawDt * 0.5);
      if (quality.slow > 1.5 && quality.level > 0.5) {
        quality.level = Math.max(0.5, quality.level - 0.2);
        quality.slow = 0;
        layout();
      }
    }
    if (!reduceMotion && !stillCapture) {
      simulate(dt);
      pose(dt);
    }
    render();
    if (first) {
      first = false;
      root.classList.add('is-live');
      if (stillCapture) {
        const aspect = COMP.width / COMP.height;
        const dpr = canvas.width / width;
        const size = place.size * STILL_MARGIN;
        const w = size * aspect * dpr;
        const h = size * dpr;
        const [crop, context] = canvas2d(Math.round(w), Math.round(h));
        context.drawImage(canvas, (place.x - size * aspect / 2) * dpr, (place.y - size / 2) * dpr, w, h, 0, 0, w, h);
        window.__diniStill = crop.toDataURL('image/png');
      }
    }
    if (visible && !reduceMotion && !stillCapture && !document.hidden) requestFrame();
  }

  function requestFrame() {
    if (running || lost) return;
    running = true;
    requestAnimationFrame(frame);
  }

  canvas.addEventListener('webglcontextlost', (event) => {
    event.preventDefault();
    lost = true;
    root.classList.remove('is-live');
  });
  canvas.addEventListener('webglcontextrestored', () => {
    canvas.remove();
    still.remove();
    root.classList.remove('is-live', 'is-placed');
    mount(root);
  });

  new ResizeObserver(() => {
    layout();
    requestFrame();
  }).observe(root);
  if (document.fonts) {
    document.fonts.ready.then(() => {
      layout();
      pagesDirty = true;
      requestFrame();
    });
  }
  new IntersectionObserver((entries) => {
    visible = entries.some((entry) => entry.isIntersecting);
    if (visible) {
      clock.getDelta();
      requestFrame();
    }
  }).observe(root);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden && visible) {
      clock.getDelta();
      requestFrame();
    }
  });
}

for (const root of document.querySelectorAll('[data-dw-hero-art]')) mount(root);
