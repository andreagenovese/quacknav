"""pages.py: the two explainer pages of the pilot work, from the code's own runs.

    pages.py TRACE.json... [--out docs/pages] [--artifact DIR]

- navigation-flow.html: how the duck decides, from boot to arrival —
  homecoming, exploration, go_to with the stick or the pilot — as three
  flowcharts (mermaid) with the code's thresholds.
- three-ways.html: one generated scenario walked three ways (the learned
  pilot, the stick of go_to, the exploration's travel), from `rl_trace`
  JSON files, drawn on the floor plan with a timeline that replays them.

`--out` writes standalone HTML documents (mermaid from jsDelivr, fonts from
Google Fonts); `--artifact` also writes the same pages without the document
wrapper, as claude.ai artifacts want them.
"""
import argparse
import json
import os

FONTS = '<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,600&family=Source+Sans+3:wght@400;600&family=JetBrains+Mono:wght@400;600&display=swap">'

TOKENS = """
:root{
 --bg:#f4f5f1; --surface:#ffffff; --fg:#1d2420; --muted:#5c6660; --line:#d6dbd4;
 --wall:#3b423e; --furn:#9aa39c; --unm:#d9822b; --hole:#141816; --floor:#fbfcf9;
 --pilot:#1f7a8c; --stick:#b0402f; --expl:#6a4fb3; --accent:#c77d12; --model:#2f6f8f; --ok:#2f7d4f;
 --display:"Fraunces",Georgia,serif; --body:"Source Sans 3","Segoe UI",system-ui,sans-serif; --mono:"JetBrains Mono",ui-monospace,Menlo,monospace;
}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){
 --bg:#121614; --surface:#1a1f1c; --fg:#e5ebe6; --muted:#a0aba4; --line:#303833;
 --wall:#c9d1cb; --furn:#59625c; --unm:#e8963e; --hole:#000000; --floor:#1e2420;
 --pilot:#4fb6c9; --stick:#e0715f; --expl:#a58ef0; --accent:#e8a33c; --model:#6fb3d4; --ok:#6cc28f; color-scheme:dark}}
:root[data-theme="dark"]{
 --bg:#121614; --surface:#1a1f1c; --fg:#e5ebe6; --muted:#a0aba4; --line:#303833;
 --wall:#c9d1cb; --furn:#59625c; --unm:#e8963e; --hole:#000000; --floor:#1e2420;
 --pilot:#4fb6c9; --stick:#e0715f; --expl:#a58ef0; --accent:#e8a33c; --model:#6fb3d4; --ok:#6cc28f; color-scheme:dark}
body{background:var(--bg);color:var(--fg);font:16px/1.55 var(--body);margin:0}
.wrap{max-width:1180px;margin:0 auto;padding-inline:16px;padding-block:28px 56px;display:grid;gap:24px}
h1,h2{font-family:var(--display);font-weight:600;margin:0;text-wrap:balance}
h1{font-size:clamp(1.8rem,4vw,2.5rem);line-height:1.15} h2{font-size:1.45rem}
p{margin:0;max-width:72ch} .lead{color:var(--muted)}
code{font-family:var(--mono);font-size:.88em}
.note{font-size:.92rem;color:var(--muted);border-left:3px solid var(--accent);padding:6px 12px;background:var(--surface)}
"""

FLOW_BODY = r"""
<div class="wrap">
 <header style="display:grid;gap:10px">
  <div class="kicker">quack-navd · branch rl-nav</div>
  <h1>How the duck decides</h1>
  <p class="lead">From power-on to arrival: waking up in a known house (homecoming), mapping by frontiers (exploration), and walking to a point (<code>go_to</code>) with the stick or the learned pilot. Diamonds are decisions; blue nodes are the models consulted; red nodes are the shields. Thresholds are the code's.</p>
 </header>
 <section class="models">
  <div class="model"><b>maploc</b><span>5 cm occupancy map and pose: a frame a second, the pose every 50 ms. Says whether the pose is trusted, whether the duck is seated or down.</span></div>
  <div class="model"><b>cliff guard</b><span>The 8 × 8 depth sensor at 15 Hz projected on the floor: obstacles in the lane, drops (holes), odometry.</span></div>
  <div class="model"><b>Dijkstra planner</b><span>Costmap from the map + the books + walked lanes; walls inflated by 0.12 m, unknown three times dearer than known floor.</span></div>
  <div class="model"><b>the books</b><span>What the duck met that the map does not have: drops (r 0.10 m) and obstacles (r 0.05 m). Dijkstra treats them as walls.</span></div>
  <div class="model"><b>pilot MLP</b><span>Only with <code>QK_RL_POLICY</code>: 351 values in, one of 9 moves out. It picks the step, never the route.</span></div>
  <div class="model"><b>gait model</b><span>0.12 m/s at vx 0.3, 0.65 rad/s per unit of yaw: where a step will take the body.</span></div>
 </section>
 <section class="part">
  <h2>1 · Homecoming, at power-on</h2>
  <p>The duck is usually switched on somewhere else in the house: it first tries to recognise a saved map, then decides whether to navigate or go on exploring.</p>
  <div class="diagram"><pre class="mermaid">
flowchart TD
  H0([power-on]) --> H1[wait 15 s]
  H1 --> H2{any saved map?}
  H2 -- no --> HEND1([nothing to come home to: stand down])
  H2 -- yes --> H3[load the newest]
  H3 --> H4[[walk and look: 6 s stands, quarter turns, legs toward free space]]
  H4 --> H5{pose confirmed within 60 s?}
  H5 -- yes --> H16[take the map back]
  H5 -- no --> H6{map frozen, or resume_explore?}
  H6 -- yes --> H7[[search three times longer]]
  H7 -- found --> H16
  H7 -- not found --> HEND2([stand down: the duck does not know where it is])
  H6 -- no --> H9[a fresh map]
  H9 --> H10[explore, up to 30 min]
  H10 --> H11[[every 60 s: map_match against the saved maps]]
  H11 --> H12{score ≤ 0.16, margin and overlap ok?}
  H12 -- no --> H11
  H12 -- yes --> H13{3 answers agreeing within 0.30 m?}
  H13 -- no --> H11
  H13 -- yes --> H14[adopt the saved map, confirm the pose]
  H14 --> H15{was the house already complete?}
  H15 -- yes --> HNAV([frozen map: ready for go_to])
  H15 -- no --> X0
  H16 --> H17{frozen or complete?}
  H17 -- yes --> HNAV
  H17 -- no, resume_explore --> X0([explore on · part 2])
  classDef model stroke:#2f6f8f,stroke-width:2px;
  classDef endn stroke:#2f7d4f,stroke-width:2px;
  class HEND1,HEND2,HNAV,X0 endn;
  class H11 model;
</pre></div>
  <div class="facts"><code>start_delay 15 s</code><code>boot_search 60 s</code><code>recognize_every 60 s</code><code>adopt_max_score 0.16</code><code>adopt_asks 3</code><code>explore_max 1800 s</code></div>
 </section>
 <section class="part">
  <h2>2 · Exploration</h2>
  <p>A loop that picks the cheapest frontier between known and unknown floor, travels there on the go_to loop (part 3), stands to look and writes the books, until only slivers are left.</p>
  <div class="diagram"><pre class="mermaid">
flowchart TD
  X0([map_explore]) --> XA{pose trusted?}
  XA -- no, the duck was moved --> XR[[relocalise, up to 240 s]] --> XA
  XA -- yes --> X2{stop, budget, or battery under 25 %?}
  X2 -- yes --> XE([save the session and its progress])
  X2 -- no --> X3{pose believable?}
  X3 -- no --> X4[3 s stand · lost over 180 s: fail] --> X2
  X3 -- yes --> X5{pose jumped, or map and sensor disagree?}
  X5 -- yes --> X6[settling stands, or a 360° panorama] --> X2
  X5 -- no --> X8[every 180 s back on known floor, to close the loop]
  X8 --> X9[[frontiers on the map + books · planner]]
  X9 --> X10{a frontier of 20 cells or more?}
  X10 -- no, 12 passes with nothing new --> XE
  X10 -- yes --> X11{pick: the cheapest not refused}
  X11 -- none --> X12[unsealing ladder: forget refusals and far bumps, never drops] --> X2
  X11 -- one --> X13{within 0.30 m of it?}
  X13 -- yes --> X14[6 s stand: the map grows, drops go on the books · an unnamed area: ask where we are] --> X2
  X13 -- no --> X15[[travel to the frontier · part 3, budget 60 + 30 s/m, max 300 s]]
  X15 -- failed --> X16[frontier refused] --> X2
  X15 -- ok --> X2
  classDef model stroke:#2f6f8f,stroke-width:2px;
  classDef endn stroke:#2f7d4f,stroke-width:2px;
  class XE,X0 endn;
  class X9,XR model;
</pre></div>
  <div class="facts"><code>LOST_PATIENCE 180 s</code><code>BIG_FRONTIER 20 cells</code><code>SLIVER_PATIENCE 12</code><code>ARRIVE 0.30 m</code><code>FRONTIER_STOP 6 s</code><code>STUCK_MAX 3</code></div>
 </section>
 <section class="part">
  <h2>3 · go_to: the journey, leg by leg</h2>
  <p>Dijkstra owns the route, on the map and the books; it is replanned from the pose at every leg, or kept while fresh and the books are unchanged. The stick or the pilot only picks the leg's move. Whatever the duck meets that the map does not have — a hole the guard sees, a thing it pushed against or the sensor sees in the lane — goes on the books, and the next plan goes round it.</p>
  <div class="diagram"><pre class="mermaid">
flowchart TD
  G0([robot.go_to]) --> G0a{a map, a trusted pose, a way there?}
  G0a -- no --> GEND0([refused, with the reason])
  G0a -- yes --> G2{stop, time budget, or battery?}
  G2 -- yes --> GEND1([end])
  G2 -- no --> G3{pose trusted?}
  G3 -- no --> G3b[6 s stand · over 180 s: fail] --> G2
  G3 -- yes --> G4{within 0.25 m of the goal?}
  G4 -- yes --> G4b[6 s stand and check again] --> GOK([arrived])
  G4 -- no --> G5[pose stands: every 20 s or 1.5 m, and when something is in the lane within 0.5 m]
  G5 --> G6{route still good? under 30 s old, books unchanged, off it under 0.40 m}
  G6 -- yes --> G9
  G6 -- no --> G7[[Dijkstra: map + books + lanes]]
  G7 -- no way --> G8[forget near bumps, never drops · after 3: fail] --> G2
  G7 -- a route --> G9{pilot loaded and not stuck?}
  G9 -- no --> S0{3 legs that did not move the body, or barely: scuffs?}
  S0 -- yes --> SB[book the obstacle where the sensor sees it, else at the nose] --> S2
  S0 -- no --> S1{route over 0.6 rad off the nose?}
  S1 -- yes --> S2[turn in place, closed on odometry]
  S1 -- no --> GUARD
  G9 -- yes --> P1[[observe: route, 1.4 s of depth frames, 1.6 m of map, last moves → MLP → one of 9 moves]]
  P1 --> P2{allowed by the shields?}
  P2 -- back-off toward unknown or a drop --> PR[refused: 0.6 s stand]
  P2 -- step into something, or pushing on --> PB[refused: the obstacle goes on the books]
  P2 -- step across a booked drop --> PR
  P2 -- 2 refusals or 4 turns in a row --> S0
  P2 -- yes --> GUARD
  GUARD{a true hole in the step's lane?}
  GUARD -- yes --> GH[1.5 s stand · the hole goes on the books · turn away] --> G7
  GUARD -- no --> GS[walk: a 0.6 s step, a turn, a back-off or a wait]
  S2 --> G13
  PR --> G13
  PB --> G7
  SB --> G7
  GS --> G13[every 0.4 m walked: a 2 s stand for the mapper]
  G13 --> G2
  classDef model stroke:#2f6f8f,stroke-width:2px;
  classDef shield stroke:#a3392b,stroke-width:2px;
  classDef endn stroke:#2f7d4f,stroke-width:2px;
  class G7,P1 model;
  class P2,PR,PB,GUARD,GH shield;
  class GOK,GEND0,GEND1,G0 endn;
</pre></div>
  <div class="facts"><code>GOAL_ARRIVE 0.25 m</code><code>KEEP_ROUTE 30 s</code><code>STAND_EVERY 0.4 m</code><code>TURN_FIRST 0.6 rad</code><code>STALLS 3 (scuff: under 40 % of a step)</code><code>BOOK_STAND 1.5 s</code><code>pilot → stick: 2 refusals or 4 turns</code></div>
  <p class="note">The two links back to Dijkstra — a hole the guard sees, an obstacle the stick pushed against — are the fixes of 2026-10-06. Before them, on a go_to, a hole seen while walking never reached the books (they take drops from standing frames, and only the exploration stood first), and a body sliding along a box was never "stalled"; Dijkstra sent the duck back the same way, 31 times at casa_ingombra's stairwell on the MuJoCo twin.</p>
 </section>
</div>
"""

FLOW_CSS = """
.kicker{font:600 .75rem/1 var(--mono);letter-spacing:.08em;text-transform:uppercase;color:var(--accent)}
.models{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:12px}
.model{border:1px solid var(--line);border-left:3px solid var(--model);padding:12px 14px;background:var(--surface);display:grid;gap:4px;min-width:0}
.model b{font-family:var(--mono);font-size:.85rem;color:var(--model)}
.model span{font-size:.92rem;color:var(--muted)}
.part{display:grid;gap:14px}
.diagram{background:var(--surface);border:1px solid var(--line);padding:16px;overflow-x:auto}
.diagram pre.mermaid{margin:0;min-width:640px;background:transparent}
.facts{display:flex;flex-wrap:wrap;gap:8px}
.facts code{font-size:.82rem;padding:3px 8px;border:1px solid var(--line);background:var(--surface)}
"""

TW_CSS = """
.tabs{display:flex;flex-wrap:wrap;gap:8px}
.tabs button{font:600 .9rem var(--body);padding:8px 14px;border:1px solid var(--line);background:var(--surface);color:var(--fg);cursor:pointer}
.tabs button[aria-pressed="true"]{border-color:var(--accent);box-shadow:inset 0 -3px 0 var(--accent)}
.tabs button:focus-visible,.ctrl button:focus-visible,.ctrl input:focus-visible,.run:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.stage{display:grid;grid-template-columns:minmax(0,1fr) 330px;gap:18px;align-items:start}
@media (max-width:860px){.stage{grid-template-columns:1fr}}
.plan{background:var(--surface);border:1px solid var(--line);padding:10px;min-width:0}
canvas{width:100%;height:auto;display:block}
.ctrl{display:flex;gap:10px;align-items:center;padding-top:10px}
.ctrl button{font:600 .9rem var(--body);padding:6px 14px;border:1px solid var(--line);background:var(--bg);color:var(--fg);cursor:pointer;min-width:84px}
.ctrl input{flex:1;min-width:0;accent-color:var(--accent)}
.ctrl output{font:.85rem var(--mono);font-variant-numeric:tabular-nums;min-width:58px;text-align:right}
.runs{display:grid;gap:10px}
.run{background:var(--surface);border:1px solid var(--line);border-left:4px solid var(--c);padding:10px 12px;display:grid;gap:4px;cursor:pointer}
.run[aria-pressed="false"]{opacity:.45}
.run .top{display:flex;justify-content:space-between;gap:8px;align-items:baseline}
.run .name{font:600 1rem var(--body);color:var(--c)}
.pill{font:600 .72rem var(--mono);letter-spacing:.04em;text-transform:uppercase;padding:2px 7px;border:1px solid currentColor}
.ok{color:var(--ok)}.bad{color:var(--stick)}.mid{color:var(--accent)}
.nums{display:grid;grid-template-columns:repeat(3,1fr);gap:4px;font:.85rem var(--mono);font-variant-numeric:tabular-nums;color:var(--muted)}
.nums .v{display:block;font:600 1rem var(--mono);color:var(--fg)}
.why{font-size:.85rem;color:var(--muted)}
.legend{display:flex;flex-wrap:wrap;gap:6px 16px;font-size:.86rem;color:var(--muted)}
.legend span{display:inline-flex;align-items:center;gap:6px}
.sw{width:14px;height:10px;display:inline-block;border:1px solid var(--line)}
"""

TW_BODY = r"""
<div class="wrap">
 <header style="display:grid;gap:10px">
  <h1>Three ways to walk</h1>
  <p class="lead">One scenario, one map, one goal, walked by the learned pilot, by the stick of today's <code>go_to</code>, and by the exploration's own way of travelling to a frontier — in the quack-rl simulator, through quack-navd's real journey loop. Orange things were put down after the map was made: the map does not know them.</p>
 </header>
 <div class="tabs" id="tabs" role="group" aria-label="Scenario"></div>
 <div class="stage">
  <div class="plan">
   <canvas id="cv" width="1400" height="1000" aria-label="Floor plan with the three tracks"></canvas>
   <div class="ctrl"><button id="play" type="button">Play</button><input id="t" type="range" min="0" max="100" step="0.2" value="0" aria-label="Time"><output id="tv">0 s</output></div>
  </div>
  <div class="runs" id="runs"></div>
 </div>
 <div class="legend">
  <span><i class="sw" style="background:var(--wall)"></i>walls and furniture on the map</span>
  <span><i class="sw" style="background:var(--unm)"></i>not on the map</span>
  <span><i class="sw" style="background:var(--hole)"></i>hole</span>
  <span>✕ bump</span><span>☠ tipped over</span><span>● booked drop</span><span>■ booked obstacle</span><span>- - Dijkstra's last route</span>
 </div>
 <p class="note">The exploration's travel is drawn for reference: it is built to reach frontiers, not a user's goal. It stands 3 s every 0.4 m, so it often runs out of the journey's time; and it gives up a goal within 0.20 m of a booked drop. Tip-overs follow the model calibrated on the MuJoCo twin (about one in 130 bumps against furniture, more often against thin legs): they are random, as there. Moving people and pets are drawn where they started.</p>
 <p class="note" id="gen"></p>
</div>
<script>
const DATA = __DATA__;
const COL = {pilot:'--pilot', stick:'--stick', exploration:'--expl'};
const LABEL = {pilot:'Learned pilot', stick:'Stick (go_to)', exploration:'Exploration travel'};
const OUT = {Arrived:['arrived','ok'], ArrivedOff:['arrived, off','mid'], Fell:['fell','bad'], Timeout:['out of time','mid'], Failed:['gave up','mid']};
let sc = 0, on = {pilot:true, stick:true, exploration:true}, t = 0, playing = false, last = 0, view;
const cv = document.getElementById('cv'), cx = cv.getContext('2d');
const css = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
function el(tag, cls, text){ const e=document.createElement(tag); if(cls) e.className=cls; if(text!==undefined) e.textContent=text; return e; }
function tabs(){ const box=document.getElementById('tabs'); box.replaceChildren();
 DATA.forEach((s,i)=>{const b=el('button',null,s.title); b.type='button'; b.setAttribute('aria-pressed', i===sc); b.onclick=()=>{sc=i; setup(); t=tmax(); document.getElementById('t').value=t; draw();}; box.appendChild(b);}); }
function tmax(){ return Math.max(...DATA[sc].runs.map(r=>r.track.length? r.track[r.track.length-1][0]:0)); }
function runs(){ const box=document.getElementById('runs'); box.replaceChildren();
 DATA[sc].runs.forEach(r=>{ const o=OUT[r.outcome]||[r.outcome,'mid']; const d=el('div','run');
  d.tabIndex=0; d.setAttribute('role','button'); d.setAttribute('aria-pressed', on[r.name]); d.style.setProperty('--c', css(COL[r.name]));
  const top=el('div','top'); top.append(el('span','name',LABEL[r.name]), el('span','pill '+o[1],o[0]));
  const nums=el('div','nums');
  [[r.secs+' s','time'],[r.path+' m','walked'],[String(r.bumps),'bumps']].forEach(([v,k])=>{ const s=el('span'); s.append(el('span','v',v), document.createTextNode(k)); nums.appendChild(s); });
  const n = k => r.events.filter(e=>e[0]===k).length; const bits=[r.reason];
  if(n('guard')) bits.push(`guard turned it from a hole ${n('guard')}×`);
  if(n('bump_book')+n('shield_book')) bits.push(`${n('bump_book')+n('shield_book')} obstacles booked`);
  if(n('stick_takes')) bits.push(`stick took ${n('stick_takes')} legs`);
  d.append(top, nums, el('div','why',bits.join(' · ')));
  const tog=()=>{on[r.name]=!on[r.name]; d.setAttribute('aria-pressed', on[r.name]); draw();};
  d.onclick=tog; d.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();tog();}};
  box.appendChild(d); }); }
function setup(){ tabs(); runs(); const b=DATA[sc].world.bounds, m=0.25;
 const W=b[1]-b[0]+2*m, H=b[3]-b[2]+2*m; const s=1400/W; cv.width=1400; cv.height=Math.round(H*s);
 view={x0:b[0]-m, y1:b[3]+m, s:s}; const ti=document.getElementById('t'); ti.max=Math.ceil(tmax());
 document.getElementById('gen').textContent = `Scenario ${DATA[sc].family} ${DATA[sc].seed} (level 3), pilot ${DATA[sc].pilot}; regenerate with scripts/rl/pages.py.`; }
const X = x => (x-view.x0)*view.s, Y = y => (view.y1-y)*view.s;
function rect(r,fill,stroke){ const a=[X(r[0]),Y(r[3]),(r[1]-r[0])*view.s,(r[3]-r[2])*view.s]; if(fill){cx.fillStyle=fill; cx.fillRect(...a);} if(stroke){cx.strokeStyle=stroke; cx.lineWidth=2; cx.strokeRect(...a);} }
function at(track, t){ if(!track.length) return null; let i=0; while(i<track.length-1 && track[i+1][0]<=t) i++; return track[i]; }
function draw(){ const s=DATA[sc], w=s.world; cx.fillStyle=css('--floor'); cx.fillRect(0,0,cv.width,cv.height);
 w.holes.forEach(h=>rect(h, css('--hole')));
 w.rects.forEach(r=>{ if(!r[5]) return; rect(r, r[4]? (Math.min(r[1]-r[0],r[3]-r[2])<0.15? css('--wall'):css('--furn')) : css('--unm')); });
 w.rects.forEach(r=>{ if(r[4] && !r[5]){ cx.setLineDash([6,5]); rect(r, null, css('--furn')); cx.setLineDash([]);} });
 w.posts.forEach(p=>{ cx.fillStyle = p[3]? css('--wall'):css('--unm'); cx.beginPath(); cx.arc(X(p[0]),Y(p[1]),Math.max(4,p[2]*view.s),0,7); cx.fill(); });
 w.movers.forEach(m=>{ cx.strokeStyle=css('--unm'); cx.lineWidth=2; cx.setLineDash([4,4]); cx.beginPath(); cx.arc(X(m[0]),Y(m[1]),m[2]*view.s,0,7); cx.stroke(); cx.setLineDash([]); });
 cx.strokeStyle=css('--accent'); cx.lineWidth=3; cx.beginPath(); cx.arc(X(s.goal[0]),Y(s.goal[1]),0.25*view.s,0,7); cx.stroke();
 cx.fillStyle=css('--accent'); cx.font=`600 ${Math.round(view.s*0.13)}px ${css('--body')}`; cx.fillText('goal', X(s.goal[0])+0.27*view.s, Y(s.goal[1])+5);
 cx.fillText('start', X(s.start[0])+0.15*view.s, Y(s.start[1])-0.15*view.s);
 s.runs.forEach(r=>{ if(!on[r.name]) return; const c=css(COL[r.name]);
  if(r.route.length>1){ cx.strokeStyle=c; cx.globalAlpha=.45; cx.setLineDash([8,7]); cx.lineWidth=2; cx.beginPath(); r.route.forEach((p,i)=> i? cx.lineTo(X(p[0]),Y(p[1])) : cx.moveTo(X(p[0]),Y(p[1]))); cx.stroke(); cx.setLineDash([]); cx.globalAlpha=1; }
  r.books.forEach(b=>{ cx.fillStyle=c; cx.globalAlpha=.45; if(b[2]>=0.1){ cx.beginPath(); cx.arc(X(b[0]),Y(b[1]),5,0,7); cx.fill(); } else { cx.fillRect(X(b[0])-5,Y(b[1])-5,10,10); } cx.globalAlpha=1; });
  const pts=r.track.filter(p=>p[0]<=t); cx.strokeStyle=c; cx.lineWidth=3.5; cx.lineJoin='round'; cx.beginPath(); pts.forEach((p,i)=> i? cx.lineTo(X(p[1]),Y(p[2])) : cx.moveTo(X(p[1]),Y(p[2]))); cx.stroke();
  r.contacts.filter(k=>k[0]<=t).forEach(k=>{ const big=k[1]==='tip'||k[1]==='hole'; cx.fillStyle=c; cx.font=`700 ${Math.round(view.s*(big?0.22:0.12))}px ${css('--body')}`; cx.fillText(big?'☠':'✕', X(k[2])-6, Y(k[3])+6); });
  const p=at(r.track,t); if(p){ const x=X(p[1]), y=Y(p[2]), R=0.11*view.s; cx.fillStyle=c; cx.beginPath(); cx.arc(x,y,R,0,7); cx.fill();
   cx.strokeStyle=css('--floor'); cx.lineWidth=3; cx.beginPath(); cx.moveTo(x,y); cx.lineTo(x+Math.cos(p[3])*R*1.3, y-Math.sin(p[3])*R*1.3); cx.stroke(); }
 });
 document.getElementById('tv').textContent = Math.round(t)+' s'; }
function tick(ts){ if(!playing) return; const dt=(ts-last)/1000; last=ts; t=Math.min(tmax(), t+dt*12); document.getElementById('t').value=t; draw(); if(t>=tmax()){playing=false; document.getElementById('play').textContent='Play'; return;} requestAnimationFrame(tick); }
document.getElementById('play').onclick=()=>{ playing=!playing; document.getElementById('play').textContent= playing?'Pause':'Play'; if(playing){ if(t>=tmax()) t=0; last=performance.now(); requestAnimationFrame(tick);} };
document.getElementById('t').oninput=e=>{ t=+e.target.value; draw(); };
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', ()=>{runs(); draw();});
setup(); t=tmax(); document.getElementById('t').value=t; draw();
</script>
"""

TITLES = {
    "doorway": "A doorway, and a box put down since",
    "clutter": "A cluttered room",
    "mixed": "A mixed house with a hole",
    "stairwell": "A passage beside a stairwell",
    "corners": "Narrow corners",
    "movers": "Things that move",
    "low": "Low furniture",
}


def compact(path, pilot_name):
    d = json.load(open(path))
    s, w = d["scenario"], d["scenario"]["world"]
    r2 = lambda v: round(v, 3)
    world = {
        "bounds": w["bounds"],
        "rects": [[r2(r["x0"]), r2(r["x1"]), r2(r["y0"]), r2(r["y1"]), int(r["mapped"]), int(r["present"]), int(r["low"]), r2(r["height"])] for r in w["rects"]],
        "posts": [[r2(p["x"]), r2(p["y"]), r2(p["r"]), int(p["mapped"])] for p in w["posts"]],
        "holes": [[r2(h["x0"]), r2(h["x1"]), r2(h["y0"]), r2(h["y1"]), int(h["booked"])] for h in w["holes"]],
        "movers": [[r2(m["x"]), r2(m["y"]), r2(m["r"])] for m in w["movers"]],
    }
    runs = []
    for r in d["runs"]:
        res = r["result"]
        runs.append({
            "name": r["name"], "outcome": res["outcome"], "reason": res["reason"], "secs": round(res["secs"]), "path": round(res["path_m"], 1),
            "bumps": res["bumps"], "tipped": res["tipped"],
            "track": [[round(t, 1), r2(x), r2(y), round(a, 2)] for t, x, y, a in r["track"]],
            "contacts": [[round(c[0], 1), c[1], r2(c[2]), r2(c[3])] for c in r["contacts"]],
            "events": [[e["kind"], r2(e["x"]), r2(e["y"])] for e in r["events"]],
            "books": [[r2(b[0]), r2(b[1]), b[2]] for b in r["books"]],
            "route": [[r2(p[0]), r2(p[1])] for p in r["route"]],
        })
    fam = s["family"]
    return {"title": TITLES.get(fam, fam), "family": fam, "seed": s["seed"], "pilot": pilot_name, "start": s["start"], "goal": s["goal"], "world": world, "runs": runs}


def page(title, css, body, mermaid, standalone):
    head = f"<title>{title}</title>\n{FONTS}\n<style>{TOKENS}{css}</style>\n"
    if not standalone:
        return head + body
    mm = ('<script src="https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.min.js"></script>\n'
          "<script>mermaid.initialize({startOnLoad:true, theme: matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'default'});</script>\n") if mermaid else ""
    return f'<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<meta name="viewport" content="width=device-width, initial-scale=1">\n{head}</head>\n<body>\n{body}\n{mm}</body>\n</html>\n'


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("traces", nargs="+")
    ap.add_argument("--out", default="docs/pages")
    ap.add_argument("--artifact")
    ap.add_argument("--pilot-name", default="v3-r6")
    args = ap.parse_args()
    data = json.dumps([compact(p, args.pilot_name) for p in args.traces], separators=(",", ":"))
    tw_body = TW_BODY.replace("__DATA__", data)
    for d, standalone in [(args.out, True), (args.artifact, False)]:
        if not d:
            continue
        os.makedirs(d, exist_ok=True)
        open(os.path.join(d, "navigation-flow.html"), "w").write(page("How the duck decides", FLOW_CSS, FLOW_BODY, True, standalone))
        open(os.path.join(d, "three-ways.html"), "w").write(page("Three ways to walk", TW_CSS, tw_body, False, standalone))
        print("wrote", d)


if __name__ == "__main__":
    main()
