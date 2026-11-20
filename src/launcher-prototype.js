"use strict";
// Local interaction prototype. All accounts, paths and records live in this tab.
const $ = selector => document.querySelector(selector);
const pathA = "D:\\Games\\Genshin Impact Game";
const pathB = "E:\\原神\\Genshin Impact Game";
const accountName = "风起地";
const uidA = "100000001";
const uidB = "100000002";
const pages = ["首页", "游戏管理", "米游社", "角色资料"];
const glyphs = ["⌂", "▤", "☼", "✧"];
const tabs = ["抽卡记录", "成就进度"];
const scenes = {
  ready: "日常启动 · 已就绪", one: "首次使用 · 找到一个目录", many: "首次使用 · 找到多个目录",
  empty: "首次使用 · 未找到游戏", download: "正在下载安装", broken: "下载中断",
  guest: "尚未授权", expired: "授权失效", retry: "签到失败"
};
const dialog = $("#dialog");
let s;
let noticeTimer;
function reset(scene) {
  s = {scene, page: "首页", path: pathA, candidates: [], task: "", percent: 0, target: pathA,
    auth: true, sign: "待核对奖励", qr: "", uid: uidA, tab: tabs[0],
    records: {[uidA]: [["菲谢尔", "祭礼弓"], ["风与牧歌的城邦"]], [uidB]: [[], []]}};
  if (["one","many","empty","download","broken"].includes(scene)) s.path = "";
  if (scene === "one") s.candidates = [pathA];
  if (scene === "many") s.candidates = [pathA,pathB];
  if (scene === "download" || scene === "broken") {
    s.task = scene === "broken" ? "下载中断" : "正在下载";
    s.percent = 42;
  }
  if (["guest","expired","retry"].includes(scene)) s.page = "米游社";
  if (scene === "guest") s.auth = false;
  if (scene === "expired") s.sign = "授权失效";
  if (scene === "retry") s.sign = "签到失败";
  $("#notice").textContent = "";
  render();
}
function esc(value) {
  return String(value).replace(/[&<>"']/g, c => ({"&":"&amp;", "<":"&lt;", ">":"&gt;", '"':"&quot;", "'":"&#39;"})[c]);
}
function btn(action, label, style = "") {
  return `<button type="button" class="btn ${style}" data-do="${action}">${esc(label)}</button>`;
}
function go(page) { s.page = page; render(); window.scrollTo(0,0); }
function notify(message) {
  clearTimeout(noticeTimer);
  $("#notice").textContent = message;
  noticeTimer = setTimeout(() => { $("#notice").textContent = ""; }, 3600);
}
function modal(title, content) {
  dialog.innerHTML = `<h2 id="dialog-title">${title}</h2>${content}`;
  dialog.showModal();
  (dialog.querySelector("[autofocus]") || dialog.querySelector("select,input,button"))?.focus();
}
const cancel = '<button type="button" class="btn" data-do="close" autofocus>取消</button>';
function folder(kind) {
  const install = kind === "install";
  modal(install ? "选择安装位置" : "选择已有游戏",
    `<label class="field">${install ? "安装位置（父目录）" : "游戏目录"}<input id="folder" value="${esc(install ? "D:\\Games" : pathA)}" required></label>
     <small class="muted">演示目录，可编辑路径。</small><div class="actions">${cancel}${btn(install ? "start" : "save-path", install ? "开始下载" : "使用此目录", "primary")}</div>`);
}
function taskActions() {
  return btn(s.task === "正在下载" ? "pause" : "resume", s.task === "正在下载" ? "暂停下载" : "继续下载");
}
function taskPanel() {
  return `<h2>${s.task} · <span data-percent>${s.percent}%</span></h2><p class="path">${esc(s.target)}</p>
    <progress max="100" value="${s.percent}"></progress>${s.task === "下载中断" ? "<p>网络连接中断，可从已下载的位置继续。</p>" : ""}
    <div class="actions">${taskActions()}</div>`;
}
function directoryActions() {
  if (!s.candidates.length) return `<div class="actions">${btn("browse","选择已有游戏","primary")}${btn("install","下载安装")}</div>`;
  return `<fieldset><legend>找到以下游戏目录</legend>${s.candidates.map((p,i) =>
    `<label class="directory"><input type="radio" name="candidate" value="${i}" ${i===0 ? "checked" : ""}> ${esc(p)}</label>`).join("")}</fieldset>
    <div class="actions">${btn("candidate","使用此目录","primary")}${btn("browse","选择其他目录")}</div>`;
}
function quickCard(index,title,detail) {
  return `<button class="card" data-page="${pages[index]}"><span class="tile-icon" aria-hidden="true">${glyphs[index]}</span>
    <span><strong>${title}</strong><small>${esc(detail)}</small></span><span class="arrow">↗</span></button>`;
}
function home() {
  const state = s.task || (s.path ? "游戏目录已就绪" : "配置游戏目录");
  return `<div class="title"><h1>首页</h1></div>
    <article class="hero"><div class="hero-copy"><small>GENSHIN IMPACT</small><h2>原神</h2>
    <span class="status"><i class="dot"></i>${state}${s.task ? ` · <span data-percent>${s.percent}%</span>` : ""}</span>
    ${s.task ? `<div class="actions">${taskActions()}${btn("game","任务详情")}</div>` : s.path ?
    `<div class="actions">${btn("launch","▶　普通启动","primary")}<button class="link" data-do="game">管理游戏 ↗</button></div>` : directoryActions()}
    </div><div class="hero-art" role="img" aria-label="湖畔蒙德城与远山风景"></div></article>
    <h2 class="section-name">快捷入口</h2><div class="cards">${quickCard(2,"社区签到",s.auth ? `${accountName} · ${s.sign}` : "扫码添加米哈游账号")}
    ${quickCard(3,"角色资料",`当前 UID ${s.uid}`)}</div>`;
}
function game() {
  return `<div class="title"><h1>游戏管理</h1></div><section class="panel">${s.task ? taskPanel() : s.path ?
    `<h2>游戏目录</h2><p class="path">${esc(s.path)}</p><details><summary>更换目录</summary>
    <div class="actions">${btn("discover","自动查找")}${btn("browse","浏览文件夹")}</div>
    <label class="field">手动输入游戏目录<input id="manual" value="${esc(s.path)}"></label><div class="actions">${btn("manual","使用此目录")}</div></details>` :
    `<h2>配置游戏目录</h2>${directoryActions()}`}</section>`;
}
function qrPanel() {
  const message = {new:"等待扫码", confirm:"请在米游社确认", expired:"二维码已过期", wrong:"请使用原账号重新扫码"}[s.qr];
  const acts = s.qr === "confirm" ? btn("authorize","模拟确认授权","primary") :
    ["expired","wrong"].includes(s.qr) ? btn("qr","刷新二维码","primary") :
    btn("scanned","模拟扫码","primary") + btn("qr-expire","模拟过期");
  return `<section class="panel"><h2>${s.auth ? "续期账号 · " + accountName : "扫码添加米哈游账号"}</h2><div class="qr" aria-label="演示码">▦</div>
    <p role="status">${message}</p><div class="actions">${acts}${s.qr==="confirm" && s.auth ? btn("wrong","模拟其他账号确认") : ""}${btn("qr-cancel","取消")}</div></section>`;
}
function community() {
  const title = `<div class="title"><h1>米游社</h1>${s.auth ? `<strong>米哈游账号 · ${accountName}</strong>` : ""}</div>`;
  if (s.qr) return title + qrPanel();
  if (!s.auth) return title + `<section class="panel"><h2>社区签到</h2><p>扫码授权后，可查看今日签到结果与奖励。</p>
    <div class="actions">${btn("qr","扫码添加米哈游账号","primary")}</div></section>`;
  const status = {"待核对奖励":"今日已签到 · 奖励待确认","已领取":"今日已签到 · 奖励已领取","授权失效":"授权已失效","签到失败":"签到失败 · 网络连接中断"}[s.sign];
  const act = {"待核对奖励":btn("reward","检查奖励领取"),"已领取":"","授权失效":btn("qr","重新扫码"),"签到失败":btn("retry","重试社区签到")}[s.sign];
  const days = Array.from({length:7},(_,i) => {
    const d = new Date(Date.now() - (6-i)*86400000);
    const label = new Intl.DateTimeFormat("zh-CN",{month:"numeric",day:"numeric",timeZone:"Asia/Shanghai"}).format(d);
    return `<div class="day">${i===6 ? "今天" : label}<br>${i===6 ? esc(s.sign) : "无记录"}</div>`;
  });
  return title + `<section class="panel"><h2>${status}</h2><div class="actions">${act}</div></section>
    <section class="panel"><h2>社区签到奖励日历</h2><div class="calendar">${days.join("")}</div></section>
    <section class="panel"><details><summary>管理授权</summary><div class="actions">${btn("check","检查授权")}${btn("qr","重新扫码")}${btn("remove","移除账号","danger")}</div></details></section>`;
}
function profiles() {
  const idx = tabs.indexOf(s.tab), records = s.records[s.uid][idx];
  return `<div class="title"><div class="row"><h1>角色资料</h1><select id="uid" aria-label="当前游戏 UID">${Object.keys(s.records).map(uid => `<option ${uid===s.uid?"selected":""} value="${uid}">UID ${uid}</option>`).join("")}</select></div>
    ${btn("create","创建 UID 数据空间")}</div><section class="panel">
    <small class="muted">资料类型预览 · 演示数据</small><div class="tabs">${tabs.map((t,i)=>`<button data-tab="${i}" aria-pressed="${s.tab===t}">${t}</button>`).join("")}</div>
    <h2>${s.tab}</h2>${records.length ? records.map(r=>`<div class="list-row"><strong>${esc(r)}</strong><small>UID ${s.uid}</small></div>`).join("") : `<p>此 UID 暂无${s.tab}。</p>`}
    <div class="actions">${btn("import","导入示例记录")}</div></section>
    <section class="panel"><details><summary>资料管理</summary><p>管理 UID ${s.uid} 的本地记录。</p><div class="actions">${btn("clear","删除历史记录","danger")}</div></details></section>`;
}
function render() {
  $("#crumb").textContent = s.page;
  $("#navigation").innerHTML = pages.map((page,i)=>`<button data-page="${page}" ${page===s.page?'aria-current="page"':""}><span class="glyph" aria-hidden="true">${glyphs[i]}</span><span class="nav-label">${page}</span></button>`).join("");
  $("#transfer").innerHTML = s.task ? `<button class="task-link" data-do="game">↓　${s.task} · <span data-percent>${s.percent}%</span>${s.task==="下载中断"?" · 查看原因":""}</button>` : "";
  $("#page").innerHTML = ({"首页":home,"游戏管理":game,"米游社":community,"角色资料":profiles})[s.page]();
}
function confirmAction(kind) {
  const remove = kind === "remove";
  const title = remove ? `移除账号 ${accountName}` : `删除 UID ${s.uid} 的历史记录`;
  modal(title, `<p>${remove ? "移除后，自动签到需要重新授权。本地角色资料会保留。" : `将清除 UID ${s.uid} 所有类别的本地历史。UID 数据空间与米游社授权会保留。`}</p>
    <div class="actions">${cancel}${btn(remove?"remove-ok":"clear-ok",title,"danger")}</div>`);
}
function readPath(id) {
  const input = $(id);
  input.setCustomValidity(input.value.trim() ? "" : "请输入目录");
  return input.reportValidity() ? input.value.trim() : "";
}
function savePath(path) {
  if (!path) return;
  s.path = path; s.candidates = [];
  if (dialog.open) dialog.close();
  render(); notify("默认游戏目录已保存");
}
const actions = {
  game:()=>go("游戏管理"), launch:()=>notify("已请求启动游戏（演示）"),
  browse:()=>folder("browse"), install:()=>folder("install"), close:()=>dialog.close(),
  "save-path":()=>savePath(readPath("#folder")), manual:()=>savePath(readPath("#manual")),
  candidate:()=>savePath(s.candidates[Number($("input[name=candidate]:checked").value)]),
  discover:()=>modal("找到游戏目录",`<label class="field">选择默认目录<select id="found">${[pathA,pathB].map(p=>`<option>${esc(p)}</option>`).join("")}</select></label><div class="actions">${cancel}${btn("found","使用此目录","primary")}</div>`),
  found:()=>savePath($("#found").value),
  start:()=>{
    const parent = readPath("#folder");
    if (!parent) return;
    s.target = parent.replace(/[\\/]+$/,"") + "\\Genshin Impact Game";
    s.task = "正在下载"; s.percent = 0; dialog.close(); go("游戏管理");
  },
  pause:()=>{s.task="已暂停";render();}, resume:()=>{s.task="正在下载";render();},
  qr:()=>{s.qr="new";render();}, scanned:()=>{s.qr="confirm";render();},
  "qr-expire":()=>{s.qr="expired";render();}, wrong:()=>{s.qr="wrong";render();},
  "qr-cancel":()=>{s.qr="";render();},
  authorize:()=>{s.auth=true;s.qr="";s.sign="待核对奖励";render();notify("米游社授权已保存");},
  reward:()=>{s.sign="已领取";render();},retry:()=>{s.sign="待核对奖励";render();},
  check:()=>notify(s.sign==="授权失效"?"授权已失效，请重新扫码":"当前账号授权有效"),
  remove:()=>confirmAction("remove"), clear:()=>confirmAction("clear"),
  "remove-ok":()=>{s.auth=false;dialog.close();render();notify("已移除米游社授权");},
  "clear-ok":()=>{s.records[s.uid]=[[],[]];dialog.close();render();notify(`已清除 UID ${s.uid} 的本地历史`);},
  create:()=>modal("创建 UID 数据空间",`<label class="field">游戏 UID<input id="new-uid" inputmode="numeric" pattern="[1-9][0-9]{8,9}" required placeholder="输入 9 或 10 位游戏 UID"></label><div class="actions">${cancel}${btn("create-ok","创建并切换","primary")}</div>`),
  "create-ok":()=>{
    const input=$("#new-uid");
    input.setCustomValidity(s.records[input.value] ? "此 UID 数据空间已存在" : "");
    if (!input.reportValidity()) return;
    s.uid=input.value;s.records[s.uid]=[[],[]];dialog.close();render();
  },
  import:()=>{
    const records=s.records[s.uid][tabs.indexOf(s.tab)];
    const item=s.tab===tabs[0]?"西风猎弓":"风神的宠儿";
    if (!records.includes(item)) records.push(item);
    render();notify("已合并示例记录");
  },
  scenes:()=>modal("演示场景",`<label class="field">选择起始状态<select id="scenario">${Object.entries(scenes).map(([k,v])=>`<option value="${k}" ${k===s.scene?"selected":""}>${v}</option>`).join("")}</select></label>
    <p class="muted">应用场景会重置本次演示数据。所有操作均为本地模拟。</p><div class="actions">${cancel}${btn("apply-scene","应用场景","primary")}</div>`),
  "apply-scene":()=>{const scene=$("#scenario").value;dialog.close();reset(scene);}
};
document.addEventListener("click",event=>{
  const b=event.target.closest("button");
  if (!b) return;
  if (b.dataset.page) go(b.dataset.page);
  else if (b.dataset.tab !== undefined) {s.tab=tabs[Number(b.dataset.tab)];render();}
  else actions[b.dataset.do]?.();
});
document.addEventListener("change",event=>{
  if (event.target.id==="uid") {s.uid=event.target.value;render();$("#uid").focus();}
});
setInterval(()=>{
  if (s.task !== "正在下载") return;
  s.percent=Math.min(100,s.percent+2);
  if (s.percent===100) {s.path=s.target;s.task="";render();notify("安装完成，已保存默认目录");return;}
  document.querySelectorAll("[data-percent]").forEach(el=>{el.textContent=`${s.percent}%`;});
  document.querySelectorAll("progress").forEach(el=>{el.value=s.percent;});
},1500);
const requestedScene=new URLSearchParams(location.search).get("scene");
reset(Object.hasOwn(scenes,requestedScene) ? requestedScene : "ready");
