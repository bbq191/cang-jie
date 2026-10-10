/* 字体列表的名字：家族名 + 本地化名 + 多文件时"N 个文件（字重/样式）"——同一家族的常规/半粗/粗体等文件合成一行，
   悬停列出文件名（原来只标"×N"，看不出是什么，2026-10-07 用户问）。阅读字体、界面字体两张列表共用。 */
const fontLabel=it=>{const ex=it.extra||{},f=ex.files||[];
  return esc(it.name)+(ex.names&&ex.names.cn&&ex.names.cn!==it.name?' <span class="small">'+esc(ex.names.cn)+'</span>':'')
    +(f.length>1?` <span class="small" title="${esc(f.join('\n'))}">${esc(T('assets.fonts.filesCount',{count:f.length}))}</span>`:'')};
const cjkBadge=p=>p==null?'':`<span class="badge ${p>=80?'on':(p>=8?'':'off')}" title="${T('common.cjkCoverageTitle')}">${T('common.cjkCoverage',{pct:p})}</span>`;

/* 服务 tab（按注册表出现）。key = 注册的服务名。service→seg（AREA）不再在这里手搓一份——
   那正是 gateway/src/manage.rs::MODULES 表已声明的唯一事实源，网关在 GET /api/services 的每一项上带回
   `seg`（2026-10-09 起；此前网页为它另取一次 /api/manage），见 app.js 启动代码里的 SEG 变量：手搓的映射会跟 MODULES 改名/新增
   悄悄脱节，SSE 事件的 svc 就对不上、「其他」里子面板的事件驱动刷新会静默失效。 */
const TABS={
 'note-serve':{titleKey:'tab.notes',render:renderNotes},
 'font-serve':{render:renderFonts},
 'wallpaper-serve':{render(sec){assetTab(sec,'/api/wallpapers',{
   hint:T('wallpaper.hint'),
   header:`<label class="field">${T('wallpaper.rotateLabel')}</label><div class="row"><select id="wpmode" style="max-width:12em"><option value="sequential">${T('wallpaper.mode.sequential')}</option><option value="random">${T('wallpaper.mode.random')}</option><option value="fixed">${T('wallpaper.mode.fixed')}</option></select><span id="wpst" class="small"></span></div>`,
   icon:'🖼',label:T('wallpaper.dropLabel'),accept:IMG_EXT,btn:T('wallpaper.btn'),
   onRender:async(sec,refresh)=>{const st=await j('/api/wallpapers/status');const sel=$('#wpmode',sec);if(st.ok){sel.value=st.mode;const nv=st.native||{};$('#wpst',sec).textContent=T('wallpaper.status',{current:st.current||T('wallpaper.none'),nativeState:nv.enabled?T('wallpaper.nativeEnabled'):T('wallpaper.nativeDisabled'),restartNote:nv.restartPending?T('wallpaper.restartNote'):''})}
     sel.onchange=async()=>{if((await sendT('/api/wallpapers/mode','PUT',{mode:sel.value},'wallpaper.switchFailed')).ok!==false)refresh()}},
   row:(it,left,right,refresh)=>{const cur=(it.extra||{}).current;
     // alt="" 原来把这张图当装饰性处理，但壁纸缩略图本身就是内容（"这张壁纸长什么样"），屏幕阅读器
     // 会整个跳过（2026-09-09 审计发现）；文件名本身当描述最直接，跟右边视觉上显示的文字一致。
     left.style.cssText='display:flex;align-items:center;gap:.6em'; // 缩略图固定在左、长文件名在右侧自己折行，不绕着图片流
     left.innerHTML=`<img src="/api/wallpapers/${encodeURIComponent(it.name)}" alt="${esc(T('wallpaper.thumbAlt',{name:it.name}))}" loading="lazy" style="height:3.4em;flex:none;border-radius:.3em;border:1px solid var(--line)"><span style="min-width:0">${esc(it.name)}</span>`;
     right.insertAdjacentHTML('beforeend',`<span>${fmtB(it.bytes)}</span>`+(cur?`<span class="badge on">${T('wallpaper.current')}</span>`:''));
     if(!cur){right.appendChild(btn(T('wallpaper.use'),async()=>{if((await sendT('/api/wallpapers/current','PUT',{name:it.name},'wallpaper.setFailed')).ok!==false)refresh()}));
       right.appendChild(delBtn(T('wallpaper.deleteConfirm',{name:it.name}),'/api/wallpapers/'+encodeURIComponent(it.name),refresh))}}})}}
};

/* 「其他 → xochitl」：阅读字体（进阅读器字体菜单、当中文缺字回退）+ 界面字体（2026-10-07：只给 xochitl 界面用，
   不进阅读器菜单、不当阅读回退；xovi 扩展 enhance/ui-font 与 shelf/xovi/ui-font-tokens.qmd 开机读选择，改完整机重启）。 */
function renderFonts(sec){
  assetTab(sec,'/api/fonts',{
   title:T('assets.fonts.title'),
   hint:T('assets.fonts.hint'),
   header:`<div id="fbchain" class="opt-note" style="display:none"></div>
      <div class="row"><label class="toggle"><input type="checkbox" id="embold" checked> ${T('assets.fonts.emboldenLabel')}</label> <span class="small">${T('assets.fonts.emboldenHint')}</span></div>`,
   icon:'🔤',label:T('assets.fonts.dropLabel'),accept:FONT_EXT,btn:T('assets.fonts.btn'),listTitle:T('assets.fonts.listTitle'),
   onRender:async(sec,refresh,fl)=>{
     // 中文缺字回退链：覆盖率≥8% 的中文字体，按覆盖率降序
     const cjk=(fl.items||[]).filter(it=>((it.extra||{}).cjkPct||0)>=8).sort((a,b)=>(b.extra.cjkPct||0)-(a.extra.cjkPct||0));
     const fb=$('#fbchain',sec);fb.style.display='';fb.innerHTML=cjk.length?T('assets.fonts.fallbackChain',{chain:cjk.map(it=>`${esc(it.name)} <span class="small">${esc(it.extra.cjkPct)}%</span>`).join(' → ')}):T('assets.fonts.noCjkWarn');
     const fst=await j('/api/fonts/status');const eb=$('#embold',sec);if(fst.ok){eb.checked=!!fst.emboldenCjkFallback;bindToggle(eb,'/api/fonts/config','emboldenCjkFallback')}},
   row:(it,left,right,refresh)=>{const ex=it.extra||{};
     left.innerHTML=fontLabel(it);
     right.insertAdjacentHTML('beforeend',cjkBadge(ex.cjkPct)+(ex.fontconfigRef?`<span title="${T('assets.fonts.fallbackRefTitle')}">⚠</span>`:''));
     right.appendChild(delBtn(T('assets.fonts.deleteConfirm',{name:it.name,filesNote:ex.files&&ex.files.length>1?T('assets.fonts.filesNote',{count:ex.files.length}):'',suffix:ex.fontconfigRef?T('assets.fonts.deleteSuffixFallback'):T('assets.fonts.deleteSuffixNormal')}),'/api/fonts/'+encodeURIComponent(it.name),refresh))}});
  const readRefresh=sec.refresh;
  const card=el('div',{class:'card'});
  card.innerHTML=`<h2>${T('assets.uiFont.title')}</h2><p class="small">${T('assets.uiFont.hint')}</p>
    <label class="field" for="uisans">${T('assets.uiFont.sansLabel')}</label><div class="row"><select id="uisans" style="max-width:18em"></select></div>
    <label class="field" for="uiserif">${T('assets.uiFont.serifLabel')}</label><div class="row"><select id="uiserif" style="max-width:18em"></select></div>
    <div id="uist" class="opt-note" style="display:none"></div>
    ${upHtml('🔤',T('assets.fonts.dropLabel'),FONT_EXT,T('assets.uiFont.btn'))}
    <h3>${T('assets.uiFont.listTitle')}</h3><ul class="list" id="uil"></ul>`;
  sec.appendChild(card);
  const sans=$('#uisans',card),serif=$('#uiserif',card),st=$('#uist',card);
  const fill=(sel,native,items,cur)=>{sel.innerHTML='';sel.appendChild(el('option',{value:'',text:T('assets.uiFont.native',{name:native})}));
    items.forEach(it=>sel.appendChild(el('option',{value:it.name,text:it.extra&&it.extra.names&&it.extra.names.cn&&it.extra.names.cn!==it.name?`${it.name}（${it.extra.names.cn}）`:it.name})));
    sel.value=items.some(it=>it.name===cur)?cur:''};
  const show=d=>{st.style.display=d.restartNeeded?'':'none';st.textContent=d.restartNeeded?T('assets.uiFont.restartNeeded'):''};
  const uiRefresh=async()=>{const d=await j('/api/fonts/ui');const items=d.items||[];
    fill(sans,'reMarkable Sans',items,d.sans);fill(serif,'reMarkable Serif',items,d.serif);show(d);
    fillList($('#uil',card),items,(it,left,right)=>{const ex=it.extra||{};
      left.innerHTML=fontLabel(it);
      right.insertAdjacentHTML('beforeend',cjkBadge(ex.cjkPct)+`<span>${fmtB(it.bytes)}</span>`);
      right.appendChild(delBtn(T('assets.uiFont.deleteConfirm',{name:it.name}),'/api/fonts/ui/'+encodeURIComponent(it.name),uiRefresh))},T('assets.uiFont.empty'))};
  const save=async()=>{sans.disabled=serif.disabled=true;
    const r=await sendT('/api/fonts/ui/select','PUT',{sans:sans.value,serif:serif.value},'common.saveFailed');
    sans.disabled=serif.disabled=false;if(r.ok!==false){show(r);toast(T('assets.uiFont.saved'),'ok')}else uiRefresh()};
  sans.onchange=save;serif.onchange=save;
  uploader($('.up',card),'/api/fonts/ui',FONT_EXT,uiRefresh);
  uiRefresh();
  sec.refresh=()=>Promise.all([readRefresh(),uiRefresh()]);
}

/* 资产页模板（字体 / 壁纸）：说明 + 可选头部 + 上传区 + 列表。o: {title?,hint,header?,icon,label,accept,btn,listTitle?,onRender?(sec,refresh,data),row(it,left,right,refresh)} */
function assetTab(sec,api,o){sec.innerHTML=`<div class="card">${o.title?`<h2>${o.title}</h2>`:''}<p class="${o.title?'small':'lead'}">${o.hint}</p>${o.header||''}
  ${upHtml(o.icon,o.label,o.accept,o.btn)}
  <h3>${o.listTitle||T('assets.installedDefault')}</h3><ul class="list" id="al"></ul></div>`;
  const refresh=async()=>{const d=await j(api);fillList($('#al',sec),d.items||[],(it,left,right)=>o.row(it,left,right,refresh),T('assets.emptyHint',{btn:o.btn}));if(o.onRender)o.onRender(sec,refresh,d)};
  uploader($('.up',sec),api,o.accept,refresh);
  refresh();sec.refresh=refresh}

/* 「其他」顶层 tab（2026-09-10 用户重排首层标签：传书/笔记/其他/管理）：xochitl(font-serve)/壁纸(wallpaper-serve)
   原来各自独立的顶层 tab 降一级，包进这个 tab 当二级子标签——各服务的 render() 原样复用，只是换个挂载点。
   只装了其中一部分时，subnav 只列已装的那几个（笔记 tab 本身不在这里——note-serve 单独占「其他」前面那个固定位置）。
   （原来的 KOReader 子标签随 2026-09-29 设备卸载 KOReader 撤掉。） */
function renderOther(sec,svcs,segOf){
  const items=[{name:'font-serve',icon:'🔤',label:'xochitl'},{name:'wallpaper-serve',icon:'🖼️',label:T('tab.wallpaper')}]
    .filter(it=>svcs.some(s=>s.name===it.name));
  sec.innerHTML=`<div class="subnav">${items.map((it,i)=>`<button${i===0?' class="on"':''}>${it.icon} ${it.label}</button>`).join('')}</div>
    ${items.map((it,i)=>`<div class="subpanel${i===0?' on':''}" id="other-${it.name}"></div>`).join('')}`;
  items.forEach(it=>TABS[it.name].render($('#other-'+it.name,sec)));
  const pane=it=>$('#other-'+it.name,sec);
  sec.refresh=()=>Promise.all(items.map(it=>{const c=pane(it);return c&&c.refresh&&c.refresh()}));
  /* 事件只刷发事件的那个服务的子面板（字体/壁纸各自 2 个请求），不再几块一起重取——壁纸每次休眠轮换都会发事件。
     认不出来源（没有映射）时退回整块刷新。 */
  sec.onEvent=ev=>{const it=items.find(x=>segOf(x.name)===ev.svc);const c=it&&pane(it);if(c&&c.refresh)refreshSec(c);else refreshSec(sec)};
  subtabs(sec);
}
