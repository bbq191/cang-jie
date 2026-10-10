// 只放 key 名，DashScope/OpenAI/Gemini/DeepSeek 本身是厂商专名不翻，括注里的中文说明才走 T()（同样是
// 顶层 const 只存 key、真正查找挪到调用点的规则，见 T() 头注）。
const PROVIDER_NAMES={dashscope:'models.provider.dashscope',openai:'models.provider.openai',gemini:'models.provider.gemini',deepseek:'models.provider.deepseek'};
/* 模型管理卡片（点 2「彻底重做」，「管理」tab 用，transcribe/mind 共用同一套 UI，2026-09-08 第二轮反馈；
   2026-09-08 又一轮反馈：厂家/模型拆成两级下拉，别把七八个不同厂家的模型糊在一个框里选）：
   第一级「厂家」下拉（DashScope/OpenAI/Gemini/DeepSeek/自定义），第二级「模型」下拉只列选中厂家的
   模型——选厂家会自动定位到该厂家的第一个模型（服务端 `preset` 立即原子切换，不用再点一次确认）；
   选"自定义"隐藏模型下拉、露出手填 model/baseUrl。key 按厂商分开存、脱敏后只剩「删除」，没配 key
   时才给输入框——不允许在已有 key 时直接改写覆盖，逼着"先删再填"；切换预置不会丢别的厂商已存的 key
   （服务端按 provider 分格）。每个模型自己的用量+花费一行（`usageByModel`，花费＝用户自填单价×token，
   没填单价不显示金额，见 config.rs 模块文档"花费不做官方定价表"——第三方定价常变，不猜）；`showAuto`
   给 transcribe 用，多一个"合书自动转写"开关（模型服务级别的设置，原来在「整理」页的折叠层已经去掉，
   见 renderNotes）。返回一个 refresh 函数，挂到 tab 的 sec.refresh 上，切回这个 tab 时数据不过期。 */
function mountModelPanel(root,seg,title,icon,showAuto){
  const card=el('div',{class:'card',style:'width:100%;margin:0'});
  card.innerHTML=`<h3 style="margin-top:0">${icon} ${title}</h3>
    <div class="row">
      <div style="flex:1;min-width:11em"><label class="field">${T('models.vendorLabel')}</label><select data-vendor style="width:100%"></select></div>
      <div style="flex:1;min-width:11em" data-modelbox><label class="field">${T('models.modelLabel')}</label><select data-preset style="width:100%"></select></div>
    </div>
    <div class="row" data-custom hidden>
      <input type="text" data-model placeholder="${T('models.customModelPlaceholder')}" style="max-width:11em">
      <input type="text" data-url placeholder="${T('models.customUrlPlaceholder')}" style="flex:1;min-width:12em">
      <button class="btn" data-savecustom>${T('models.saveCustomBtn')}</button>
    </div>
    <label class="field">${T('models.apiKeyLabel')}</label>
    <div class="row" data-keyrow></div>
    ${showAuto?`<div class="row"><label class="toggle"><input type="checkbox" data-auto> ${T('models.autoTranscribeToggle')}</label></div>`:''}
    <label class="field">${T('models.priceLabel')}</label>
    <div class="row" data-pricerow>
      <input type="number" step="0.001" min="0" data-pricein placeholder="${T('models.priceInPlaceholder')}" style="max-width:6em">
      <input type="number" step="0.001" min="0" data-priceout placeholder="${T('models.priceOutPlaceholder')}" style="max-width:6em">
      <button class="btn" data-pricesave>${T('models.savePriceBtn')}</button>
    </div>
    <label class="field">${T('models.usageLabel')}</label>
    <div class="tblwrap" data-usagewrap><table class="cmp"><thead><tr><th>${T('models.usage.colModel')}</th><th>${T('models.usage.colCalls')}</th><th>${T('models.usage.colTokens')}</th><th>${T('models.usage.colCost')}</th></tr></thead><tbody data-usagebody></tbody></table></div>
    <div class="small" data-stat style="margin-top:.3em;overflow-wrap:anywhere"></div>`;
  root.appendChild(card);
  const vendorSel=card.querySelector('[data-vendor]'),modelBox=card.querySelector('[data-modelbox]'),presetSel=card.querySelector('[data-preset]'),customBox=card.querySelector('[data-custom]'),modelInp=card.querySelector('[data-model]'),urlInp=card.querySelector('[data-url]'),keyRow=card.querySelector('[data-keyrow]'),stat=card.querySelector('[data-stat]'),autoBox=card.querySelector('[data-auto]'),priceIn=card.querySelector('[data-pricein]'),priceOut=card.querySelector('[data-priceout]'),usageBody=card.querySelector('[data-usagebody]');
  const cfgUrl=`/api/${seg}/config`;
  // 「改一项配置 → 失败弹 toast → 无论成败都重画」：厂家/模型/密钥/自定义/单价这几个动作共用。
  const putR=async(body,failKey)=>{await sendT(cfgUrl,'PUT',body,failKey);refresh()};
  const fmtCost=c=>c==null?T('models.noPrice'):'¥'+c.toFixed(4);
  let presets=[];
  const modelsOf=v=>presets.filter(p=>p.provider===v);
  const refresh=async()=>{
    const st=await j(`/api/${seg}/status`);
    if(st.ok===false){stat.textContent=T('models.notReady',{msg:st.message||T('models.notReadyDefault')});vendorSel.disabled=true;keyRow.innerHTML='';usageBody.innerHTML='';return}
    const c=st.config||{};
    presets=c.presets||[];
    vendorSel.disabled=false;
    const vendors=[...new Set(presets.map(p=>p.provider))];
    vendorSel.innerHTML=vendors.map(v=>`<option value="${esc(v)}">${esc(T(PROVIDER_NAMES[v])||v)}</option>`).join('')+`<option value="custom">${T('models.customVendor')}</option>`;
    const activeVendor=c.activePreset==='custom'?'custom':(presets.find(p=>p.id===c.activePreset)||{}).provider||'custom';
    vendorSel.value=activeVendor;
    const isCustom=activeVendor==='custom';
    customBox.hidden=!isCustom;modelBox.hidden=isCustom;
    if(isCustom){modelInp.value=c.model||'';urlInp.value=c.baseUrl||''}
    else{presetSel.innerHTML=modelsOf(activeVendor).map(p=>`<option value="${esc(p.id)}">${esc(p.label)}</option>`).join('');presetSel.value=c.activePreset}
    keyRow.innerHTML=c.hasKey
      ?`<span class="small">${T('models.keySaved',{key:esc(c.keyMasked||'••••')})}</span><button class="btn" data-delkey>${T('action.delete')}</button>`
      :`<input type="password" placeholder="${T('models.keyInputPlaceholder')}" data-keyinput style="flex:1;min-width:11em" autocomplete="off"><button class="btn pri" data-savekey>${T('models.saveKeyBtn')}</button>`;
    if(autoBox){autoBox.checked=!!c.auto;bindToggle(autoBox,cfgUrl,'auto')}
    const price=c.price||{inputPer1k:0,outputPer1k:0};
    priceIn.value=price.inputPer1k||'';priceOut.value=price.outputPer1k||'';
    const rows=st.usageByModel||[];
    usageBody.innerHTML=rows.length?rows.map(m=>`<tr${m.active?' style="font-weight:600"':''}><td>${esc(m.label)}${m.active?` <span class="badge on">${T('models.usage.active')}</span>`:''}</td><td>${m.calls}${m.failed?` <span style="color:var(--bad)">${T('models.usage.failedCount',{n:m.failed})}</span>`:''}</td><td>${m.promptTokens}/${m.completionTokens}</td><td>${fmtCost(m.costEstimate)}</td></tr>`).join(''):`<tr><td colspan="4" class="small">${T('models.usage.none')}</td></tr>`;
    const errRow=rows.find(m=>m.active&&m.lastError);stat.textContent=errRow?T('models.lastError',{err:errRow.lastError}):'';
    const delKeyBtn=keyRow.querySelector('[data-delkey]'),saveBtn=keyRow.querySelector('[data-savekey]');
    if(delKeyBtn)guardClick(delKeyBtn,async()=>{if(!await confirmDialog(T('models.confirmDeleteKey',{title})))return;await putR({clearKey:true},'models.deleteFailed')});
    if(saveBtn)guardClick(saveBtn,async()=>{const v=keyRow.querySelector('[data-keyinput]').value.trim();if(!v)return;await putR({apiKey:v})});
  };
  /* 选厂家：不是自定义就直接定位到该厂家第一个模型并原子切换（不用再点一次「确认」）；选自定义只切
     UI（露出手填框），真正生效要等用户填完点「保存自定义」——避免半吊子状态被当成已保存的配置发出去。 */
  vendorSel.onchange=async()=>{const v=vendorSel.value;customBox.hidden=v!=='custom';modelBox.hidden=v==='custom';
    if(v==='custom')return;
    const first=modelsOf(v)[0];if(!first)return;
    await putR({preset:first.id})};
  presetSel.onchange=async()=>{await putR({preset:presetSel.value})};
  guardClick(card.querySelector('[data-savecustom]'),async()=>{await putR({preset:'custom',model:modelInp.value.trim(),baseUrl:urlInp.value.trim()})});
  guardClick(card.querySelector('[data-pricesave]'),async()=>{await putR({price:{inputPer1k:parseFloat(priceIn.value)||0,outputPer1k:parseFloat(priceOut.value)||0}})});
  refresh();
  return refresh;
}

/* 管理台/引导（固定 tab，始终在——它是网关自身页面，不由服务注册表驱动） */
/* 「管理」二级 tab（2026-09-09 起三个，2026-09-10 加到五个）：① 基石与模块（原来就有的引导/开关/
   卸载）② 模型管理（原来挂在这页最下面，现在单独一屏，不用跟基石列表一起滚）③ 系统增强（只留真正
   "系统级"的开关，CJK 画线吸附、阅读器翻页）④ 实验室（还在打磨/覆盖面没到日常好用程度的功能：现在只有导入md文档
   可见性开关；漫画页边距开关 2026-10-07 删除）。2026-09-30 移除：电池刺客（原「系统增强」里的开关卡 + 运行时才出现的「电池刺客」
   二级 tab）与实验室里的「CJK 手写笔迹优化」开关。
   （曾在这页的 shelf push 命令卡片已随 2026-09-18 砍掉 host CLI 一并删除。）另有「设备健康」（2026-09-25）。 */
/* 系统增强/实验室开关的文案与所在子标签（键 = 网关 enhance/mod.rs TOGGLES 的 key；顶层只存 i18n 键名，T() 在渲染时查）。 */
const TOGGLE_UI={
  hlSnapCjk:{panel:'enhance',title:'manage.enhance.hlSnap.title',desc:'manage.enhance.hlSnap.desc',label:'manage.enhance.hlSnap.toggle'},
  tapPageTurn:{panel:'enhance',title:'manage.enhance.pageTurn.title',desc:'manage.enhance.pageTurn.desc',label:'manage.enhance.pageTurn.tapToggle',hint:'manage.enhance.pageTurn.tapHint'},
  notesImportMdEnabled:{panel:'lab',title:'manage.lab.importMd.title',desc:'manage.lab.importMd.desc',label:'manage.lab.importMd.toggle'},
};
/* 开关旁的加载状态徽章：`kind`（extension 扩展 / patch qmd 补丁 / web 纯网页功能）只决定说明文字，状态由网关判定（`loaded`）。 */
const LOADED_TITLE={extension:{on:'manage.loaded.onTitle',off:'manage.loaded.offTitle'},patch:{on:'manage.loaded.qmdOnTitle',off:'manage.loaded.qmdOffTitle'}};
const loadedBadge=t=>t.kind==='web'?`<span class="badge" title="${T('manage.loaded.webOnlyTitle')}">${T('manage.loaded.webOnly')}</span>`
  :t.loaded==='unknown'?`<span class="badge" title="${T('manage.loaded.noXochitlTitle')}">${T('manage.loaded.unknown')}</span>`
  :t.loaded==='on'?`<span class="badge on" title="${T(LOADED_TITLE[t.kind].on)}">${T('manage.loaded.on')}</span>`
  :t.loaded==='pending'?`<span class="badge" title="${T('manage.loaded.qmdPendingTitle')}">${T('manage.loaded.pending')}</span>`
  :`<span class="badge off" title="${T((LOADED_TITLE[t.kind]||LOADED_TITLE.extension).off)}">${T('manage.loaded.off')}</span>`;
/* 模块管理动作（start / stop / uninstall），「基石与模块」列表与「全部开启/关闭」共用。 */
const modAct=(seg,act,loud)=>(loud?sendT:jsend)('/api/manage/'+seg+'/'+act,'POST');
function renderManage(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">${T('manage.subnav.foundation')}</button><button data-sub="health">${T('manage.subnav.health')}</button><button>${T('manage.subnav.models')}</button><button>${T('manage.subnav.enhance')}</button><button>${T('manage.subnav.lab')}</button></div>
  <div class="subpanel on">
    <div class="card"><h2>${T('manage.foundation.title')}</h2><p class="lead">${T('manage.foundation.lead')}</p>
      <div class="kv small" id="found">${T('manage.foundation.checking')}</div>
      <p class="small">${T('manage.foundation.links')}</p></div>
    <div class="card"><h2>${T('manage.modules.title')}</h2>
      <p class="lead">${T('manage.modules.lead')}</p>
      <details class="cmp"><summary>${T('manage.modules.helpSummary')}</summary>
        <dl class="help">
          <dt>${T('manage.modules.help.states.dt')}</dt>
          <dd>${T('manage.modules.help.states.dd')}</dd>
          <dt>${T('manage.modules.help.toggle.dt')}</dt>
          <dd>${T('manage.modules.help.toggle.dd')}</dd>
          <dt>${T('manage.modules.help.perf.dt')}</dt>
          <dd>${T('manage.modules.help.perf.dd')}</dd>
          <dt>${T('manage.modules.help.uninstall.dt')}</dt>
          <dd>${T('manage.modules.help.uninstall.dd')}</dd>
          <dt>${T('manage.modules.help.install.dt')}</dt>
          <dd>${T('manage.modules.help.install.dd')}</dd>
        </dl></details>
      <div class="row"><button class="btn" id="allon">${T('manage.modules.allOn')}</button><button class="btn" id="alloff">${T('manage.modules.allOff')}</button></div>
      <ul class="list" id="mods"></ul></div>
  </div>
  <div class="subpanel" id="healthBox"></div>
  <!-- 意图卡（h2+lead）单独一张、跟下面的模型卡是兄弟不是父子（2026-09-10 用户要求跟「管理」页
       其它子标签统一风格——「传书·入库」「引导·基石」都是这个样子：一张说明卡起头，后面各功能
       各自一张卡平铺；改之前这里是说明卡把 #modelcards 包在里面，卡中卡，跟别处不一样）。 -->
  <div class="subpanel">
    <div class="card"><h2>${T('manage.models.title')}</h2><p class="lead">${T('manage.models.lead')}</p></div>
    <div id="modelcards" style="display:flex;flex-direction:column;gap:1em"></div>
  </div>
  <div class="subpanel" data-toggles="enhance"></div>
  <div class="subpanel" data-toggles="lab"></div>
  </div>`;
  const mvRefresh=mountModelPanel($('#modelcards',sec),'transcribe',T('manage.models.visionTitle'),'👁',true);
  const mtRefresh=mountModelPanel($('#modelcards',sec),'mind',T('manage.models.textTitle'),'✎');
  /* 基石 + 模块三态 + 系统增强开关：三个接口并行取，/api/enhance/status 只取一次（扩展加载状态在「基石」与
     「系统增强/实验室」两处都要用，原来各取一遍，每次都让网关扫一遍 /proc）。 */
  /* coalesce：开关模块后的延时刷新、服务注册表变化的 manage 事件、「全部开启/关闭」收尾几乎同时到，合成一轮（此前各发一遍 3 个请求，
     其中 /api/enhance/status 每次都让网关扫一遍 /proc）。 */
  const refresh=coalesce(async()=>{
    const [f,es,d]=await Promise.all([j('/api/foundation'),j('/api/enhance/status'),j('/api/manage')]);
    const inst=v=>badge(v?T('common.installed'):T('common.notInstalled'),v);
    const ld=(es.ok!==false&&es.loaded)||{},live=[...(ld.extensions||[]),...(ld.qmds||[])];
    $('#found',sec).innerHTML=f.ok===false?`<span>${esc(f.message)}</span>`:
      `<b>xovi</b><span>${inst(f.xovi)}</span><b>qt-resource-rebuilder</b><span>${inst(f.qrr)}</span>`
      +`<b>${T('manage.loaded.xoviLive')}</b><span>${xoviBadge(ld.xovi)}${live.length?' <span class="small">'+esc(live.join(' · '))+'</span>':''}</span>`;
    const ul=$('#mods',sec);ul.innerHTML='';(d.modules||[]).forEach(m=>{
      const [state,cls]=!m.installed?[T('common.notInstalled'),'off']:m.running?[T('manage.modules.state.on'),'on']:[T('manage.modules.state.installedOff'),''];
      const lk='manage.modules.label.'+m.seg,label=I18N[lk]?T(lk):m.label; // 语言包缺这个 seg 时兜底用后端 Rust 侧的中文 label（T() 缺 key 返回 key 本身，不能靠 ||）
      const left=el('span',{html:`${esc(label)} <span class="small">${esc(m.service)}</span> <span class="badge ${cls}">${state}</span>`});
      const right=el('span',{style:'display:flex;gap:.4em;align-items:center'});
      if(m.installed){
        const t=btn(m.running?T('manage.modules.turnOff'):T('manage.modules.turnOn'),async()=>{await modAct(m.seg,m.running?'stop':'start',true);setTimeout(refresh,600)});
        // 成功才整页重载（tab 集合变了）；失败时 sendT 已弹出原因，此前照样 0.8 秒后重载，原因一闪就没了。
        const u=btn(T('manage.modules.uninstallBtn'),async()=>{if(!await confirmDialog(T('manage.modules.confirmUninstall',{label})))return;
          if((await modAct(m.seg,'uninstall',true)).ok===false){refresh();return}
          toast(T('manage.modules.uninstalled',{label}),'ok');setTimeout(()=>location.reload(),800)});
        right.append(t,u);
      }else right.appendChild(el('span',{class:'small',html:T('manage.modules.installCmd',{only:esc(m.only)})}));
      ul.appendChild(el('li',{style:'flex-wrap:wrap'},[left,right]))});
    if(es.ok!==false)await erApply(es)});
  guardClick($('#allon',sec),async()=>{const d=await j('/api/manage');for(const m of (d.modules||[]))if(m.installed&&!m.running)await modAct(m.seg,'start');refresh()});
  guardClick($('#alloff',sec),async()=>{if(!await confirmDialog(T('manage.modules.confirmAllOff')))return;const d=await j('/api/manage');for(const m of (d.modules||[]))if(m.installed&&m.running)await modAct(m.seg,'stop');refresh()});
  /* 系统增强/实验室（Track 3，2026-09-09；实验室 2026-09-10 加）：开关表在网关 enhance/mod.rs 的 TOGGLES，
     /api/enhance/status 的 toggles 给出每个开关的开/关与"xochitl 里实际加载了没有"（2026-10-10 起由网关判定，网页不再认识
     .so/.qmd 文件名）。这里按 TOGGLE_UI 循环出卡片，写都走同一个 /api/enhance/qol。 */
  const toggleBox={},toggleBadge={};
  for(const [key,ui] of Object.entries(TOGGLE_UI)){
    const box=el('input',{type:'checkbox'}),badgeEl=el('span');
    const card=el('div',{class:'card'},[el('h3',{style:'margin-top:0',text:T(ui.title)}),el('p',{class:'small',text:T(ui.desc)}),
      el('label',{class:'toggle'},[box,' '+T(ui.label)]),' ',badgeEl].concat(ui.hint?[el('p',{class:'small',text:T(ui.hint)})]:[]));
    sec.querySelector(`[data-toggles="${ui.panel}"]`).appendChild(card);
    bindToggle(box,'/api/enhance/qol',key);toggleBox[key]=box;toggleBadge[key]=badgeEl}
  const erApply=async r=>{
    // 开关旁边标"xochitl 里实际有没有加载这个扩展/补丁"：开关只是配置，产物没加载时开了也不生效——历史上两次
    // "看着装了、其实没生效"就是这种情况。
    for(const t of r.toggles||[]){if(!toggleBox[t.key])continue;toggleBox[t.key].checked=!!t.on;toggleBadge[t.key].innerHTML=loadedBadge(t)}};
  /* 设备健康：切到这个子标签时才取数（每次切过去都取一次，网关侧有 15 秒缓存），不跟着管理页的 SSE 刷新走；
     清理那组只在它是当前二级 tab 时一起取（见 mountHealth）。 */
  const healthLoad=mountHealth($('#healthBox',sec));
  const healthNavBtn=sec.querySelector(':scope > .subnav [data-sub="health"]');
  refresh();sec.refresh=()=>Promise.all([refresh(),mvRefresh(),mtRefresh()]);subtabs(sec);
  const tabClick=healthNavBtn.onclick;healthNavBtn.onclick=()=>{tabClick();healthLoad(false)};}
