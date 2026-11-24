/** /admin — single-page admin UI (inline HTML, no build step, Bearer from localStorage). */

import type { Env } from "../worker-configuration";

const PAGE = `<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>scout admin</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body { font: 14px/1.5 system-ui, -apple-system, sans-serif; background: #0d1117; color: #e6edf3; margin: 0; padding: 24px; max-width: 1100px; margin-inline: auto; }
  h1 { font-size: 18px; margin: 0 0 4px; }
  .sub { color: #8b949e; font-size: 12px; margin-bottom: 20px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input, button, select { background: #161b22; color: #e6edf3; border: 1px solid #30363d; border-radius: 6px; padding: 6px 10px; font: inherit; }
  input:focus { outline: 1px solid #58a6ff; }
  button { cursor: pointer; } button:hover { background: #21262d; border-color: #8b949e; }
  button.primary { background: #1f6feb; border-color: #1f6feb; } button.primary:hover { background: #388bfd; }
  button.danger { color: #f85149; }
  table { width: 100%; border-collapse: collapse; margin-top: 12px; }
  th { text-align: left; color: #8b949e; font-weight: 500; font-size: 12px; padding: 6px 8px; border-bottom: 1px solid #30363d; }
  td { padding: 8px; border-bottom: 1px solid #21262d; vertical-align: top; }
  tr:hover td { background: #161b22; }
  .badge { display: inline-block; padding: 1px 8px; border-radius: 10px; font-size: 11px; }
  .ok { background: #1a3a2a; color: #3fb950; } .fail { background: #4a1f24; color: #f85149; }
  .off { background: #30363d; color: #8b949e; }
  .new { background: #1a3547; color: #58a6ff; } .changed { background: #3b2e14; color: #d29922; } .gone { background: #4a1f24; color: #f85149; }
  .muted { color: #8b949e; font-size: 12px; }
  .err { color: #f85149; font-size: 12px; max-width: 260px; overflow-wrap: break-word; }
  section { margin-top: 28px; } h2 { font-size: 15px; margin: 0 0 8px; }
  .card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 14px; }
  textarea { width: 100%; min-height: 70px; background: #0d1117; color: #e6edf3; border: 1px solid #30363d; border-radius: 6px; padding: 8px; font: 13px/1.4 ui-monospace, monospace; }
  #toast { position: fixed; bottom: 20px; right: 20px; background: #1f6feb; color: white; padding: 8px 14px; border-radius: 6px; opacity: 0; transition: opacity .3s; pointer-events: none; }
  #toast.show { opacity: 1; }
  a { color: #58a6ff; text-decoration: none; }
  .fmeta { font-size: 12px; color: #8b949e; }
  .fpayload { font-size: 12px; color: #c9d1d9; }
</style>
</head>
<body>
<h1>scout admin</h1>
<div class="sub">elsewhen-scout · Cloudflare Workers · <span id="health">…</span></div>

<div class="row" id="tokenRow" style="margin-bottom:16px">
  <input id="token" type="password" placeholder="SCOUT_TOKEN" style="width:320px">
  <button class="primary" id="saveToken">保存并连接</button>
  <span class="muted">token 只存本机 localStorage</span>
</div>

<section id="main" style="display:none">
  <section>
    <h2>源状态</h2>
    <table id="sources"><thead><tr>
      <th>源</th><th>状态</th><th>最近一轮</th><th>间隔(分)</th><th>条目</th><th>最近错误</th><th></th>
    </tr></thead><tbody></tbody></table>
  </section>

  <section>
    <div class="row" style="justify-content:space-between">
      <h2>发现（最新 <span id="fCount"></span> 条）</h2>
      <div class="row">
        <select id="fSource"><option value="">全部源</option></select>
        <select id="fKind"><option value="">全部类型</option><option>new</option><option>changed</option><option>gone</option></select>
        <button id="fReload">刷新</button>
      </div>
    </div>
    <table id="findings"><thead><tr>
      <th>时间</th><th>源</th><th>类型</th><th>条目</th><th>详情</th>
    </tr></thead><tbody></tbody></table>
  </section>

  <section>
    <h2>运行日志（最近 20）</h2>
    <table id="runs"><thead><tr>
      <th>时间</th><th>源</th><th>状态</th><th>条目/发现</th><th>错误</th>
    </tr></thead><tbody></tbody></table>
  </section>

  <section>
    <h2>Chrome Web Store 自动发现</h2>
    <div class="card">
      <div class="muted">自动从商店扩展发现页采集名称、用途简介和评分，无需填写 ID。发现页为商店推荐，首次出现不代表刚发布。</div>
      <details style="margin-top:12px"><summary>额外跟踪指定扩展（可选）</summary>
        <p class="muted">粘贴商店链接或扩展 ID，逗号或换行分隔，最多 20 个。留空仍会正常自动发现；保存后下个采集周期生效。</p>
        <textarea id="cwsIds" placeholder="粘贴 Chrome Web Store 扩展链接（可留空）"></textarea>
        <div class="row" style="margin-top:8px"><button class="primary" id="saveCws">保存额外跟踪</button><span id="cwsMsg" class="muted"></span></div>
      </details>
    </div>
  </section>
</section>
<div id="toast"></div>
<script>
const $ = (s) => document.querySelector(s);
const token = () => localStorage.getItem("scout_token") || "";
const toast = (m) => { const t = $("#toast"); t.textContent = m; t.classList.add("show"); setTimeout(() => t.classList.remove("show"), 2200); };

async function api(path, opts = {}) {
  const r = await fetch(path, { ...opts, headers: { Authorization: "Bearer " + token(), "Content-Type": "application/json", ...(opts.headers || {}) } });
  if (r.status === 401) { $("#main").style.display = "none"; $("#tokenRow").style.display = "flex"; throw new Error("unauthorized"); }
  if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error || r.status);
  return r.json();
}

function esc(s) { const d = document.createElement("div"); d.textContent = s == null ? "" : String(s); return d.innerHTML; }

function fmtTime(iso) { return iso ? iso.replace("T", " ").slice(5, 16) + " UTC" : "—"; }
function fmtPayload(p) {
  if (!p) return "";
  if (p.error) return '<span class="err">' + esc(p.error) + "</span>";
  const parts = [];
  if (p.name) parts.push("<b>" + esc(p.name) + "</b>");
  if (p.title) parts.push("<b>" + esc(p.title) + "</b>");
  if (p.price_cents != null) parts.push("$" + (p.price_cents / 100).toFixed(0));
  if (p.users != null) parts.push(esc(p.users.toLocaleString()) + " users");
  if (p.rating != null) parts.push("★" + p.rating + " (" + esc(p.rating_count ?? "") + ")");
  if (p.summary) parts.push(esc(String(p.summary).slice(0, 80)));
  if (p.link) parts.push('<a href="' + esc(p.link) + '" target="_blank">link</a>');
  else if (p.permalink) parts.push('<a href="https://gumroad.com/l/' + esc(p.permalink) + '" target="_blank">gumroad</a>');
  if (p.stars != null) parts.push("⭐ " + esc(p.stars));
  return parts.join(" · ");
}

async function loadSources() {
  const rows = await api("/admin/sources");
  const tb = $("#sources tbody"); tb.innerHTML = "";
  $("#fSource").innerHTML = '<option value="">全部源</option>';
  for (const r of rows) {
    const badge = r.enabled ? (r.last_status === "succeeded" ? '<span class="badge ok">正常</span>' : r.last_status === "failed" ? '<span class="badge fail">失败</span>' : '<span class="badge off">未运行</span>') : '<span class="badge off">已停用</span>';
    tb.insertAdjacentHTML("beforeend", '<tr>'
      + "<td><b>" + esc(r.id) + '</b><div class="muted">' + esc(r.name) + "</div></td>"
      + "<td>" + badge + "</td>"
      + "<td>" + fmtTime(r.last_run_at) + '<div class="muted">下次 ~' + (r.last_run_at ? fmtTime(new Date(Date.parse(r.last_run_at) + r.interval_minutes * 60000).toISOString()) : "下个周期") + "</div></td>"
      + '<td><input type="number" min="15" value="' + r.interval_minutes + '" style="width:70px" data-src="' + esc(r.id) + '" class="iv"></td>'
      + "<td>" + esc(r.last_items ?? "—") + "</td>"
      + '<td class="err">' + esc(r.last_error || "") + "</td>"
      + '<td><div class="row">'
      + '<button class="trig" data-src="' + esc(r.id) + '">触发</button>'
      + (r.enabled ? '<button class="danger tog" data-src="' + esc(r.id) + '" data-to="false">停用</button>' : '<button class="tog" data-src="' + esc(r.id) + '" data-to="true">启用</button>')
      + "</div></td></tr>");
    $("#fSource").insertAdjacentHTML("beforeend", "<option>" + esc(r.id) + "</option>");
  }
  tb.querySelectorAll("button.trig").forEach(b => b.onclick = async () => {
    b.disabled = true; b.textContent = "…";
    try { const r = await api("/admin/trigger", { method: "POST", body: JSON.stringify({ source: b.dataset.src }) });
      const x = r[0]; toast(x.error ? "失败: " + x.error : x.source + ": " + (x.items ?? 0) + " 条, " + (x.findings ?? 0) + " 发现");
    } catch (e) { toast("错误: " + e.message); }
    loadSources();
  });
  tb.querySelectorAll("button.tog").forEach(b => b.onclick = async () => {
    await api("/admin/sources/" + b.dataset.src, { method: "PATCH", body: JSON.stringify({ enabled: b.dataset.to === "true" }) });
    loadSources();
  });
  tb.querySelectorAll("input.iv").forEach(inp => inp.onchange = async () => {
    const v = parseInt(inp.value, 10);
    if (!(v >= 15)) { toast("间隔至少 15 分钟"); return; }
    await api("/admin/sources/" + inp.dataset.src, { method: "PATCH", body: JSON.stringify({ interval_minutes: v }) });
    toast("间隔已更新");
  });
}

async function loadFindings() {
  const src = $("#fSource").value, kind = $("#fKind").value;
  const q = new URLSearchParams({ limit: "100" });
  if (src) q.set("source", src); if (kind) q.set("kind", kind);
  const rows = await api("/findings?" + q);
  $("#fCount").textContent = rows.length;
  const tb = $("#findings tbody"); tb.innerHTML = "";
  for (const r of rows) {
    tb.insertAdjacentHTML("beforeend", "<tr>"
      + '<td class="fmeta">' + fmtTime(r.detected_at) + "</td>"
      + '<td class="fmeta">' + esc(r.source_id) + "</td>"
      + '<td><span class="badge ' + esc(r.kind) + '">' + esc(r.kind) + "</span></td>"
      + '<td class="fpayload">' + fmtPayload(r.payload) + '<div class="muted">' + esc(r.item_key.slice(0, 60)) + "</div></td>"
      + '<td class="fpayload">' + fmtPayload(r.payload) + "</td>"
      + "</tr>");
  }
}

async function loadRuns() {
  const rows = await api("/runs?limit=20");
  const tb = $("#runs tbody"); tb.innerHTML = "";
  for (const r of rows) {
    tb.insertAdjacentHTML("beforeend", "<tr>"
      + '<td class="fmeta">' + fmtTime(r.started_at) + "</td>"
      + '<td class="fmeta">' + esc(r.source_id) + "</td>"
      + "<td>" + (r.status === "succeeded" ? '<span class="badge ok">ok</span>' : '<span class="badge fail">fail</span>') + "</td>"
      + '<td class="fmeta">' + (r.items_count ?? 0) + " / " + (r.findings_count ?? 0) + "</td>"
      + '<td class="err">' + esc(r.error || "") + "</td></tr>");
  }
}

async function loadAll() {
  await Promise.all([loadSources(), loadFindings(), loadRuns()]);
  const cws = await api("/admin/settings/cws_ids");
  $("#cwsIds").value = cws.value;
  const h = await fetch("/health").then(r => r.json()).catch(() => ({}));
  $("#health").textContent = h.status === "ok" ? "健康" : "异常";
}

$("#saveToken").onclick = async () => {
  localStorage.setItem("scout_token", $("#token").value.trim());
  $("#tokenRow").style.display = "none"; $("#main").style.display = "block";
  loadAll().catch(e => toast(e.message));
};
$("#fReload").onclick = loadFindings;
$("#fSource").onchange = loadFindings;
$("#fKind").onchange = loadFindings;
$("#saveCws").onclick = async () => {
  const r = await api("/admin/settings/cws_ids", { method: "PUT", body: JSON.stringify({ value: $("#cwsIds").value }) });
  $("#cwsMsg").textContent = "已保存 " + r.ids.length + " 个额外跟踪扩展（无效输入已忽略），自动发现继续运行";
  toast("CWS 清单已保存");
};

(async () => {
  try { const h = await fetch("/health").then(r => r.json()); $("#health").textContent = h.status === "ok" ? "健康" : "异常"; } catch { $("#health").textContent = "不可达"; }
  if (token()) {
    try { await api("/admin/sources"); $("#tokenRow").style.display = "none"; $("#main").style.display = "block"; loadAll(); }
    catch { /* token 失效，留着输入框 */ }
  }
})();
</script>
</body>
</html>`;

export function adminPage(_env: Env): Response {
  return new Response(PAGE, {
    headers: { "content-type": "text/html; charset=utf-8" },
  });
}
