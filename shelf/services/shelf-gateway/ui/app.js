const $=(s,r=document)=>r.querySelector(s);
const fmtB=n=>n>1048576?(n/1048576).toFixed(1)+' MB':n>1024?(n/1024).toFixed(0)+' KB':n+' B';
/* 轻量记忆：per-viewer 便利态，隐私窗口/禁用 storage 时静默回默认 */
const LS={get(k,d){try{const v=localStorage.getItem('shelf.'+k);return v==null?d:v}catch{return d}},set(k,v){try{localStorage.setItem('shelf.'+k,v)}catch{}}};
const onUsb=/^10\.11\.99\./.test(location.hostname);
/* 格式白名单：服务端 shelf_core::formats 注入（同一份，网页 accept + 选中即拦 = 服务端上传门） */
const EXT=__EXTS__, dot=l=>l.map(e=>'.'+e);
const BOOK_EXT=dot(EXT.book), FONT_EXT=dot(EXT.font), DICT_EXT=dot(EXT.dict), IMG_EXT=dot(EXT.image);
const up=l=>l.map(e=>e.toUpperCase()).join(' / ');
/* 书籍格式三档说明（同一份白名单分档展示，不再一口气列 18 个） */
const FMT_TIERS=`<b>${up(EXT.native)}</b>：两个读器都能去 · <b>${up(EXT.convertible)}</b>：电脑 <code>shelf push</code> 可转成 EPUB 进原生，直接上传则只能加入 KOReader · <b>${up(EXT.koOnly)}</b>：只能加入 KOReader · <b>漫画不投原生</b>：AZW3/EPUB 漫画走电脑 <code>shelf push</code> 自动转 CBZ，加入 KOReader 读`;
$('#logout').onclick=e=>{e.preventDefault();fetch('/logout',{method:'POST'}).then(()=>location.href='/login')};
async function j(url,opt){const r=await fetch(url,opt);if(r.status===401){location.href='/login?next='+encodeURIComponent(location.pathname);return {ok:false,message:'未登录'}}if(r.status===403){location.href='/password';return {ok:false,message:'需先改密码'}}let d;try{d=await r.json()}catch{d={ok:false,message:'HTTP '+r.status}}if(!r.ok&&d.ok!==false)d={ok:false,message:d.message||('HTTP '+r.status)};return d}
const postJ=async(url,body)=>{const r=await j(url,{method:'POST',body:JSON.stringify(body)});if(r.ok===false)alert(r.message||'失败');return r};

/* 上传区 HTML（拖放框 + 隐藏 input + 队列 + 按钮），一处生成、各页复用；uploader() 认这个 .up 容器 */
const upHtml=(icon,label,ext,btn)=>`<div class="up"><div class="drop"><span class="big">${icon}</span>${label}</div><input type="file" multiple hidden accept="${ext.join(',')}"><ul class="q"></ul><div class="row"><button class="btn pri go">${btn}</button></div></div>`;

/* 通用上传器：逐文件一请求，进度条，逐项回执；失败项可重传，队列可逐项删/清空，顶部总进度。box=.up 容器 */
function uploader(box,urlOf,queryOf,okExt,onFinish){
  const list=$('ul.q',box), input=$('input[type=file]',box), drop=$('.drop',box), go=$('.go',box);
  let files=[], sum=null;
  const clr=document.createElement('button');clr.type='button';clr.className='btn';clr.textContent='清空';clr.onclick=()=>{files=[];render()};go.after(clr);
  const summary=()=>{if(!sum){sum=document.createElement('div');sum.className='small';sum.style.margin='.3em 0';list.parentNode.insertBefore(sum,list)}
    const ok=files.filter(f=>f.st==='ok').length,bad=files.filter(f=>f.st==='bad').length;
    sum.innerHTML=files.length?`${ok}/${files.length} 完成${bad?` · <span style="color:var(--bad)">${bad} 失败</span>`:''}`:'';};
  const render=()=>{list.innerHTML='';files.forEach(f=>{const li=document.createElement('li');li.dataset.k=f.k;li.className=f.st||'';
      li.innerHTML=`<div class="name">${f.file.name} <span class="small">${fmtB(f.file.size)}</span> <button class="btn x" type="button" title="移除" style="padding:.05em .45em;line-height:1">×</button></div><progress value="${f.st==='ok'?100:0}" max="100"></progress><div class="msg">${f.msg||'待传'}</div>`;
      li.querySelector('.x').onclick=()=>{files=files.filter(x=>x.k!==f.k);render()};list.appendChild(li)});summary()};
  const add=fl=>{for(const f of fl){const rej=okExt&&!okExt.some(e=>f.name.toLowerCase().endsWith(e));
      files.push({file:f,k:Math.random().toString(36).slice(2),rej,st:rej?'bad':'',msg:rej?('格式不收：只接受 '+okExt.join(' / ')):''})}render()};
  input.onchange=()=>{add(input.files);input.value=''};
  drop.ondragover=e=>{e.preventDefault();drop.classList.add('hi')};drop.ondragleave=()=>drop.classList.remove('hi');
  drop.ondrop=e=>{e.preventDefault();drop.classList.remove('hi');add(e.dataTransfer.files)};
  drop.onclick=()=>input.click();
  go.onclick=async()=>{go.disabled=true;clr.disabled=true;
    for(const f of files){if(f.st==='ok'||f.rej)continue;          // 成功项跳过；格式不收项不上传；失败项允许重传
      const li=list.querySelector(`li[data-k="${f.k}"]`);if(!li)continue;const pg=$('progress',li),msg=$('.msg',li);
      f.st='';li.className='';pg.value=0;msg.textContent='上传中…';
      await new Promise(res=>{const x=new XMLHttpRequest();const q=queryOf();x.open('POST',urlOf()+(q?'?'+new URLSearchParams(q):''));
        x.upload.onprogress=e=>{if(e.lengthComputable)pg.value=e.loaded/e.total*100};
        x.onload=()=>{if(x.status===401){location.href='/login';return}let d;try{d=JSON.parse(x.responseText)}catch{d={ok:false,message:'HTTP '+x.status}}
          const it=(d.items&&d.items[0])||d;f.st=it.ok?'ok':'bad';f.msg=(it.message||(it.ok?'完成':'失败'))+(d.note&&it.ok?' · '+d.note:'');li.className=f.st;msg.textContent=f.msg;pg.value=100;summary();res()};
        x.onerror=()=>{f.st='bad';f.msg='网络错误';li.className='bad';msg.textContent=f.msg;summary();res()};
        const fd=new FormData();fd.append('file',f.file);x.send(fd)})}
    go.disabled=false;clr.disabled=false;if(onFinish)onFinish()};
  return {clear(){files=[];render()}};
}

/* 二级标签：面板都由外层 render/refresh 预先填好，切换只显隐 */
function subtabs(sec){const nav=$('.subnav',sec);if(!nav)return;const btns=[...nav.children],panels=[...sec.querySelectorAll('.subpanel')];
  btns.forEach((b,i)=>b.onclick=()=>{btns.forEach(x=>x.classList.remove('on'));panels.forEach(p=>p.classList.remove('on'));b.classList.add('on');if(panels[i])panels[i].classList.add('on')});}

/* 列表渲染骨架：每项一行「左：名字等 ｜ 右：徽章/大小/按钮」；row(it,left,right,li) 填内容。字体/词典/壁纸共用 */
function fillList(ul,items,row){ul.innerHTML='';if(!items.length){ul.innerHTML='<li class="small">（空）</li>';return}
  items.forEach(it=>{const li=document.createElement('li');const left=document.createElement('span'),right=document.createElement('span');
    right.className='small';right.style.cssText='display:flex;align-items:center;gap:.4em;flex-wrap:wrap';row(it,left,right,li);li.append(left,right);ul.appendChild(li)})}
/* 删除按钮：confirm → DELETE → 刷新 */
function delBtn(msg,url,refresh){const d=document.createElement('button');d.className='btn';d.textContent='删除';
  d.onclick=async()=>{if(confirm(msg)){const r=await j(url,{method:'DELETE'});if(r.ok===false)alert(r.message);refresh()}};return d}
const cjkBadge=p=>p==null?'':`<span class="badge ${p>=80?'on':(p>=8?'':'off')}" title="中文基本区覆盖率">中文 ${p}%</span>`;

/* 决策辅助：不替用户分类（闲书/研读机器判不准），讲清母版库三步走 + 两读器各擅长；拿不准先投一个，母版还在 */
const GUIDE=`<details class="cmp"><summary>母版库怎么用？两个读器怎么选？（点开）</summary>
<dl class="help">
<dt>三步走</dt><dd>① <b>入库</b>：上传（书籍格式）/ 抓网文 / 电脑 <code>shelf push</code> / scp 进 inbox——书<b>原样</b>进母版库，不动字节。② <b>优化</b>（可选）：EPUB 点「优化」洗排版、脚注、中英文缩进（PDF 端上不动，重排走电脑）。③ <b>落库</b>：点「投入原生书库」或「加入 KOReader」。<b>母版留着</b>，随时再投另一个。读器页（xochitl / KOReader）只管各自的字体、词典，不传书。</dd>
<dt>格式</dt><dd>${FMT_TIERS}。</dd>
<dt>📖 投入原生书库（xochitl）：要做笔记、批注的书</dt><dd>目录跳转、脚注、换字体、<b>直接手写批注</b>、AI 解读。学术 / 论文 / 要划线的书放这；PDF 手写定稿也放这。</dd>
<dt>📚 加入 KOReader：消遣、查词的书</dt><dd>自由重排、<b>内置词典</b>、翻页手势。小说、漫画、外语书顺手。</dd>
<dt>拿不准放哪？</dt><dd>先投一个。母版还在，觉得不对随时再投另一个对照——<b>不用纠结"闲书还是研读"，去向你说了算</b>（侦探小说有人当消遣、有人拿来推理画线索图；漫画有人看有人学画）。</dd>
<dt>电脑 shelf push（进阶）</dt><dd>难搞的书走电脑：非标准格式转 EPUB、Calibre 深洗、PDF 论文重排。洗完<b>也落这个母版库</b>，去向一样在这里选。命令见「管理」页。</dd>
</dl></details>`;

/* 母版库「优化」档位说明（对应 /staging/optimize 的 mode） */
const OPTTABLE=`<div class="tblwrap"><table class="cmp"><thead><tr><th>档位</th><th>做什么</th><th>什么时候用</th></tr></thead><tbody>
<tr><th class="pick">清洗＋优化<br><span class="small">默认</span></th><td>全套：剥字体/字号/颜色/对齐锁 · 边距段距归零+按中英文习惯首行缩进 · 缺目录按标题自动建 · 伪 DRM 剥离 · 脚注就地内联常显 · 远程图内联 · 图片降采样 · e-ink 提对比 · 双 id 去重</td><td>绝大多数第三方书</td></tr>
<tr><th>清洗但保留段距</th><td>同上，但不动原书段间距</td><td>诗集 / 剧本 / 靠空行分节的书</td></tr>
<tr><th>只优化不清洗</th><td>只修脚注 / 图片 / 对比度 / 双 id，不碰排版和字体锁</td><td>已排好版、只想修脚注和图的书</td></tr>
</tbody></table></div>`;

/* 母版库列表。按格式门控按钮：EPUB→优化(未优化时)/投原生/加入 KO；PDF→投原生/加入 KO；其它→只能加入 KO。
   CBZ 漫画只能加入 KOReader（不投原生）；超体积门（nativeLimit 字节）的书灰掉投原生。「加入 KOReader」按 koInstalled 门控。
   opts: {items,q,fmt,st,xFolder(),kFolder(),mode(),clear(),koInstalled,nativeLimit,refresh()} */
function stagingList(ul,opts){
  ul.innerHTML='';
  const q=(opts.q||'').toLowerCase();
  const fmtOf=it=>it.format==='cbz'?'other':it.format;   // 筛选里 CBZ 归「其它」
  const items=(opts.items||[]).filter(it=>(!q||it.name.toLowerCase().includes(q))&&(!opts.fmt||fmtOf(it)===opts.fmt)&&(!opts.st||(opts.st==='1')===!!it.optimized));
  if(!items.length){ul.innerHTML='<li class="small">'+(opts.items&&opts.items.length?'（没有匹配的书）':'（母版库是空的——去「入库」把书弄进来）')+'</li>';return}
  items.forEach(it=>{const li=document.createElement('li');li.style.flexWrap='wrap';
    const fmt=it.format==='epub'?'EPUB':it.format==='pdf'?'PDF':(it.name.includes('.')?it.name.split('.').pop().toUpperCase():'其它');
    const st=it.format!=='epub'?'<span class="badge">原样</span>':it.level==='full'?'<span class="badge on">已优化</span>':it.level==='core'?'<span class="badge" title="只跑了核心遍（脚注/图片/对比度），没洗排版缩进——点「优化」补全">已优化·未清洗</span>':it.level==='old'?'<span class="badge" title="旧版本优化，点「优化」升级">旧版优化</span>':'<span class="badge">未优化</span>';
    const hint=it.format==='pdf'?' · 手写定稿放原生':it.format==='cbz'?' · 漫画：加入 KOReader 读（不投原生）':it.format==='other'?' · 原生读不了，只能加入 KOReader（想进原生用电脑 shelf push 转 EPUB）':'';
    // 落库记录徽章；落库时间早于母版 mtime（之后又优化过）→ 标「旧」，提示可重投
    const dv=it.delivered||{},stale=t=>t&&it.mtime&&t<it.mtime;
    const dl=(dv.native?` <span class="badge on" title="${stale(dv.native)?'投过，之后母版又优化过，可重投':'已投入原生书库'}">已投原生${stale(dv.native)?'·旧':''}</span>`:'')+(dv.koreader?` <span class="badge on" title="${stale(dv.koreader)?'加入过，之后母版又优化过，可重投':'已加入 KOReader'}">已加入KO${stale(dv.koreader)?'·旧':''}</span>`:'');
    // 渲染自检徽章（投原生后 book-serve 等 xochitl 渲染完核对页数；warn＝整章渲染失败的典型症状）
    const rc=dv.render,rb=!rc?'':rc.status==='ok'?` <span class="badge on" title="xochitl 渲染 ${rc.pages} 页，与正文量相符（缺省字号预期≈${rc.expected}）">渲染 ${rc.pages} 页</span>`:rc.status==='warn'?` <span class="badge off" title="xochitl 只渲染出 ${rc.pages} 页，按正文量预期≈${rc.expected} 页——整章渲染失败的症状（如同一标签双 id）；点「优化」修复后重投">⚠ 只渲染 ${rc.pages} 页 / 预期≈${rc.expected}</span>`:rc.status==='pending'?` <span class="badge" title="投原生后等 xochitl 渲染完成自动核对页数（最长 10 分钟）">渲染中…</span>`:` <span class="badge" title="10 分钟内没等到 xochitl 的渲染结果；在设备上打开这本书一次再重投可复核">未见渲染</span>`;
    li.innerHTML=`<span><b>${it.name}</b> <span class="badge">${fmt}</span> ${st}${dl}${rb} <span class="small">${fmtB(it.bytes)}${hint}</span></span>`;
    const right=document.createElement('span');right.style.cssText='display:flex;gap:.4em;flex-wrap:wrap;align-items:center';
    const btn=(t,pri,fn,dis,title)=>{const b=document.createElement('button');b.className='btn'+(pri?' pri':'');b.textContent=t;if(dis){b.disabled=true;b.title=title||''}else b.onclick=async()=>{b.disabled=true;b.textContent=t+'…';await fn();if(opts.refresh)opts.refresh()};right.appendChild(b)};
    if(it.format==='epub'&&!it.optimized)btn('优化',false,()=>postJ('/api/books/staging/optimize',{name:it.name,mode:opts.mode()}));
    // 体积门：超过 xochitl /upload 上限的书灰掉按钮（服务端同样拦），提示走电脑分卷
    const tooBig=opts.nativeLimit&&it.bytes>opts.nativeLimit;
    if(it.format==='epub'||it.format==='pdf')btn('投入原生书库',true,()=>postJ('/api/books/staging/deliver',{name:it.name,keep:!opts.clear(),folder:opts.xFolder()}),tooBig,`超过原生阅读器上传上限 ${fmtB(opts.nativeLimit)}：PDF 在电脑 shelf push 重推会自动分卷；EPUB 用 KOReader 读`);
    btn('加入 KOReader',true,async()=>{const r=await postJ('/api/koreader/books/adopt',{name:it.name,folder:opts.kFolder()});if(r.ok!==false){await postJ('/api/books/staging/mark',{name:it.name,target:'koreader'});if(opts.clear())await postJ('/api/books/staging/delete',{name:it.name})}},!opts.koInstalled,'KOReader 未安装（「管理」页看基石）');
    btn('删除',false,async()=>{if(confirm('从母版库删除 '+it.name+'？（已投到读器的不受影响）'))await postJ('/api/books/staging/delete',{name:it.name})});
    li.appendChild(right);ul.appendChild(li)});
}

/* 「传书」固定 tab = 三层架构入口：入库（所有内容源汇入）｜母版库（可选优化 → 选去向落库）。放第一位。
   读器页（xochitl / KOReader）不再有任何传书入口，只管各自的字体 / 词典。 */
function renderTransfer(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">📥 入库</button><button>📚 母版库</button></div>
  <div class="subpanel on">
    <div class="card"><h2>传书 · 入库</h2>
      <p class="lead">所有书从这里进：上传、抓网文、电脑 shelf push、scp 进 inbox。原样入库、不动字节；洗不洗、放哪读，到「母版库」再定。</p>
      ${GUIDE}
      ${onUsb?'':'<p class="opt-note">传大书建议走 USB <code>https://10.11.99.1:8778</code>，不占 Wi-Fi。</p>'}
      <h3>上传</h3>
      ${upHtml('⬆','点击或拖入书（可多选 · '+up(EXT.native)+' 及下列格式）',BOOK_EXT,'进母版库')}
      <p class="small">${FMT_TIERS}。不是书的文件（图片 / 压缩包）不收。</p>
      <h3>抓网文</h3>
      <div class="row"><input type="text" id="arturl" placeholder="https://… 文章链接（公众号 / 博客 / 新闻）" style="flex:1;min-width:12em"><button class="btn" id="artgo">抓取进母版库</button></div>
      <div class="small" id="artmsg" style="margin-top:.3em"></div>
      <p class="small">静态网页效果好；纯 JS 页面、付费墙抓不出。单篇文章（连载分章后续）。</p>
    </div>
  </div>
  <div class="subpanel">
    <div class="card"><h3 style="margin-top:0">母版库 <span class="small" id="stgcap"></span></h3>
      <div class="row"><span class="small">投原生 → 文件夹</span><select id="folderPreset" style="max-width:13em"><option value="lib">书库（默认）</option><option value="annot">批注文件夹</option><option value="custom">自定义…</option></select><input type="text" id="folder" placeholder="文件夹名" style="display:none;max-width:10em">
        <span class="small">加入 KOReader → 目录</span><input type="text" id="kfolder" list="kodirs" placeholder="留空＝根目录" style="max-width:9em"><datalist id="kodirs"></datalist></div>
      <div class="row"><span class="small">优化档位</span><select id="optmode" style="max-width:15em"><option value="auto">清洗＋优化（推荐）</option><option value="keep-spacing">清洗但保留段距（诗集 / 剧本）</option><option value="plain">只优化不清洗</option></select>
        <label class="toggle"><input type="checkbox" id="stgclear"> 投完从母版库清除</label></div>
      <details class="cmp"><summary>档位说明 · 母版为什么默认保留</summary>${OPTTABLE}<p class="small">母版保留＝同一本可再投另一个读器对照、换设备重投；不需要了手动删。</p></details>
      <div class="row"><input type="text" id="stgq" placeholder="搜书名…" style="flex:1;min-width:8em"><select id="stgfmt" style="max-width:8em"><option value="">全部格式</option><option value="epub">EPUB</option><option value="pdf">PDF</option><option value="other">其它</option></select><select id="stgst" style="max-width:8em"><option value="">全部状态</option><option value="0">未优化</option><option value="1">已优化</option></select><button class="btn" id="stgpurge" title="删除已投过读器的母版（读器里的书不受影响）">清理已落库</button></div>
      <div class="small" id="stgfree" style="margin:-.3em 0 .4em"></div>
      <ul class="list" id="stglist"></ul>
    </div>
  </div>`;
  let annotFolder='',koInstalled=false,items=[],nativeLimit=0;
  const g=id=>$('#'+id,sec);
  const xFolder=()=>{const p=g('folderPreset').value;return p==='lib'?'':p==='annot'?annotFolder:g('folder').value.trim()};
  const syncFolder=()=>{g('folder').style.display=g('folderPreset').value==='custom'?'':'none'};
  // 落库设置记在本机（per-viewer 便利态）
  [['folderPreset','fpreset','lib'],['folder','folder',''],['kfolder','kfolder',''],['optmode','optmode','auto']].forEach(([id,k,d])=>{g(id).value=LS.get(k,d);['input','change'].forEach(ev=>g(id).addEventListener(ev,()=>{LS.set(k,g(id).value);if(id==='folderPreset')syncFolder()}))});
  g('stgclear').checked=LS.get('stgclear','0')==='1';g('stgclear').onchange=()=>LS.set('stgclear',g('stgclear').checked?'1':'0');
  syncFolder();
  const render=()=>stagingList(g('stglist'),{items,q:g('stgq').value,fmt:g('stgfmt').value,st:g('stgst').value,xFolder,kFolder:()=>g('kfolder').value.trim(),mode:()=>g('optmode').value,clear:()=>g('stgclear').checked,koInstalled,nativeLimit,refresh:()=>refresh()});
  ['stgq','stgfmt','stgst'].forEach(id=>['input','change'].forEach(ev=>g(id).addEventListener(ev,render)));
  const refresh=async()=>{const [d,s,k,kb]=await Promise.all([j('/api/books/staging'),j('/api/books/status'),j('/api/koreader/status'),j('/api/koreader/books')]);
    if(s.ok&&s.annotFolder)annotFolder=s.annotFolder;nativeLimit=(s.ok&&s.nativeUploadLimitBytes)||0;koInstalled=!!(k.ok&&k.installed);
    // KOReader 现有目录 → 下拉候选（免手打错）
    g('kodirs').innerHTML=(kb.items||[]).filter(x=>x.kind==='dir').map(x=>`<option value="${x.name}">`).join('');
    if(d.ok===false){g('stglist').innerHTML=`<li class="small" style="color:var(--bad)">母版库不可用：${d.message||'book-serve 未开'}（去「管理」页开启）</li>`;g('stgcap').textContent='';return}
    items=d.items||[];const tot=items.reduce((a,b)=>a+b.bytes,0);g('stgcap').textContent=items.length?`${items.length} 本 · ${fmtB(tot)}`:'';
    const fr=d.freeBytes;const low=fr!=null&&fr<300*1048576;g('stgfree').style.color=low?'var(--bad)':'';g('stgfree').textContent=fr!=null?`设备剩余空间 ${fmtB(fr)}${low?' ⚠ 快满了：清理已落库或删不要的母版':''}`:'';
    render()};
  g('stgpurge').onclick=async()=>{const done=items.filter(it=>it.delivered&&(it.delivered.native||it.delivered.koreader));if(!done.length){alert('没有已落库的母版');return}
    if(!confirm(`删除 ${done.length} 本已投过读器的母版？（读器里的书不受影响，只是不能再重投）`))return;
    for(const it of done)await postJ('/api/books/staging/delete',{name:it.name});refresh()};
  uploader($('.up',sec),()=>'/api/books/staging',()=>({}),BOOK_EXT,()=>refresh());   // 书籍格式原样入库；选中即按 BOOK_EXT 拦
  const am=g('artmsg'),au=g('arturl'),ag=g('artgo');
  ag.onclick=async()=>{const url=au.value.trim();if(!url){am.textContent='请填链接';return}ag.disabled=true;am.style.color='';am.textContent='抓取中…（联网抽取正文，十几秒）';
    const r=await j('/api/books/staging/fetch-article',{method:'POST',body:JSON.stringify({url})});ag.disabled=false;
    am.style.color=r.ok===false?'var(--bad)':'var(--ok)';am.textContent=r.ok===false?('✗ '+(r.message||'失败')):('✓ '+r.message);if(r.ok!==false){au.value='';refresh()}};
  refresh();sec.refresh=refresh;subtabs(sec);}

/* 服务 tab（按注册表出现）。key = 注册的服务名 */
const AREA={'font-serve':'fonts','koreader-serve':'koreader','wallpaper-serve':'wallpapers','note-serve':'notes'};
const TABS={
 'note-serve':{title:'笔记',render:renderNotes},
 'font-serve':{title:'xochitl',render(sec){assetTab(sec,'/api/fonts',{
   title:'xochitl · 原生字体',
   hint:'ttf / otf → 装进 fontconfig 用户字体目录。上传后阅读器「文字与布局」菜单重开即可选，无需重启。传书在「传书」页；KOReader 的字体在 KOReader 页装。',
   header:`<div id="fbchain" class="opt-note" style="display:none"></div>
      <div class="row"><label class="toggle"><input type="checkbox" id="embold" checked> 中文加粗（墨水屏细笔画补偿）</label> <span class="small">默认开：对回退中文字体加粗，宋体在低对比墨水屏发淡时更清楚；翻书即见。</span></div>`,
   icon:'🔤',label:'点击或拖入 ttf/otf（可多选）',accept:FONT_EXT,btn:'上传字体',listTitle:'已装字体',
   onRender:async(sec,refresh,fl)=>{
     // 中文缺字回退链：覆盖率≥8% 的中文字体，按覆盖率降序
     const cjk=(fl.items||[]).filter(it=>((it.extra||{}).cjkPct||0)>=8).sort((a,b)=>(b.extra.cjkPct||0)-(a.extra.cjkPct||0));
     const fb=$('#fbchain',sec);fb.style.display='';fb.innerHTML=cjk.length?`中文缺字回退：${cjk.map(it=>`${it.name} <span class="small">${it.extra.cjkPct}%</span>`).join(' → ')}`:'⚠ 未装中文字体，正文缺字会显示方框——传一个全覆盖中文字体即可兜底。';
     const fst=await j('/api/fonts/status');const eb=$('#embold',sec);if(fst.ok){eb.checked=!!fst.emboldenCjkFallback;eb.onchange=async()=>{const r=await j('/api/fonts/config',{method:'PUT',body:JSON.stringify({emboldenCjkFallback:eb.checked})});if(r.ok===false){alert(r.message);eb.checked=!eb.checked}}}},
   row:(it,left,right,refresh)=>{const ex=it.extra||{};
     left.innerHTML=`${it.name}${ex.names&&ex.names.cn&&ex.names.cn!==it.name?' <span class="small">'+ex.names.cn+'</span>':''}${ex.files&&ex.files.length>1?' <span class="small">×'+ex.files.length+'</span>':''}`;
     right.insertAdjacentHTML('beforeend',cjkBadge(ex.cjkPct)+(ex.fontconfigRef?'<span title="界面中文回退引用">⚠</span>':''));
     right.appendChild(delBtn('删除字体 '+it.name+(ex.files&&ex.files.length>1?'（含 '+ex.files.length+' 个文件）':'')+(ex.fontconfigRef?'？\n⚠ 该字体是界面中文回退字体':'？'),'/api/fonts/'+encodeURIComponent(it.name),refresh))}})}},
 'koreader-serve':{title:'KOReader',render(sec){sec.innerHTML=`
  <div class="subnav"><button class="on">🔤 字体</button><button>📖 词典</button></div>
  <div class="subpanel on">
    <div class="card"><h2>KOReader</h2><div class="kv small" id="ks" style="margin-top:.5em">加载…</div>
      <details class="cmp"><summary>KOReader 怎么装 / 已经帮你调好了什么</summary>
      <dl class="help">
        <dt>安装</dt><dd>走官方仓库自装：先备齐基石 xovi + appload（见「管理」页），再从 <a href="https://github.com/koreader/koreader/releases" target="_blank" rel="noopener">官方 releases</a> 装 reMarkable Paper Pro（rmpp）版。</dd>
        <dt>已按 Move 屏调好（开箱即用，不用手动配）</dt><dd>本套件的 profile 贴近 xochitl 观感：中文主字体霞鹜新致宋、页边距、行距、脚注<b>底部弹窗</b>、悬挂标点、防误触、退出手势。</dd>
        <dt>书从哪来</dt><dd>在「传书」页把书入母版库，点「加入 KOReader」即可（母版库收的所有格式 KOReader 都能读）。有什么书，去 KOReader 里看。</dd>
        <dt>改配置 / 删字体后</dt><dd>KOReader 若正在跑，需<b>重启它</b>才生效（上方状态「运行中」会提示）。</dd>
      </dl></details></div>
    <div class="card"><h3 style="margin-top:0">字体（KOReader）</h3><p class="small">只装进 KOReader；原生阅读器的字体在 xochitl 页装。</p>
    ${upHtml('🔤','点击或拖入 ttf/otf（可多选）',FONT_EXT,'上传字体')}
    <h3>已装字体</h3><ul class="list" id="kf"></ul></div>
  </div>
  <div class="subpanel">
    <div class="card"><h3 style="margin-top:0">词典（KOReader）</h3><p class="small">StarDict 词典：填词典名，拖入这本词典的全部文件（${DICT_EXT.join(' / ')}）一起传。</p>
    <label class="field" for="dictname">词典名</label><input type="text" id="dictname" placeholder="如 牛津高阶 / cc-cedict">
    ${upHtml('📖','点击或拖入词典文件（可多选）',DICT_EXT,'上传词典')}
    <h3>已装词典</h3><ul class="list" id="kd"></ul></div>
  </div>`;
  const ups=sec.querySelectorAll('.up');
  uploader(ups[0],()=>'/api/koreader/fonts',()=>({}),FONT_EXT,()=>refresh());
  uploader(ups[1],()=>'/api/koreader/dicts',()=>({name:$('#dictname',sec).value.trim()}),DICT_EXT,()=>refresh());
  const refresh=async()=>{
    const [s,f,dc]=await Promise.all([j('/api/koreader/status'),j('/api/koreader/fonts'),j('/api/koreader/dicts')]);
    $('#ks',sec).innerHTML=s.ok?`<b>安装</b><span>${s.installed?'是':'否'} ${s.version?'('+s.version+')':''}</span><b>运行中</b><span>${s.running?'是（改配置 / 删字体后需重启它）':'否'}</span><b>已装</b><span>字体 ${s.fonts} 个 · 词典 ${s.dicts||0} 本</span>`:`<span>${s.message}</span>`;
    fillList($('#kf',sec),f.items||[],(it,left,right)=>{left.textContent=it.name;right.insertAdjacentHTML('beforeend',cjkBadge(it.cjkPct)+`<span>${fmtB(it.bytes)}</span>`);right.appendChild(delBtn('从 KOReader 删除 '+it.name+'？','/api/koreader/fonts/'+encodeURIComponent(it.name),refresh))});
    fillList($('#kd',sec),dc.items||[],(it,left,right)=>{left.textContent='📖 '+it.name;right.textContent=it.ifo+' 本'})};
  refresh();sec.refresh=refresh;subtabs(sec)}},
 'wallpaper-serve':{title:'壁纸',render(sec){assetTab(sec,'/api/wallpapers',{
   hint:'jpg / png 图片，自动裁到 954×1696。首张自动启用（写 xochitl.conf SleepScreenPath，首次需跑一次 xovi/start），之后换图下次休眠即生效。',
   header:`<label class="field">休眠轮换</label><div class="row"><select id="wpmode" style="max-width:12em"><option value="sequential">按顺序</option><option value="random">随机</option><option value="fixed">固定</option></select><span id="wpst" class="small"></span></div>`,
   icon:'🖼',label:'点击或拖入图片（可多选）',accept:IMG_EXT,btn:'上传',
   onRender:async(sec,refresh)=>{const st=await j('/api/wallpapers/status');const sel=$('#wpmode',sec);if(st.ok){sel.value=st.mode;const nv=st.native||{};$('#wpst',sec).textContent=`当前 ${st.current||'（无）'} · 原生休眠屏 ${nv.enabled?'已启用':'未启用（激活首张时自动写）'}${nv.restartPending?' · 需跑一次 xovi/start 生效':''}`}
     sel.onchange=async()=>{await j('/api/wallpapers/mode',{method:'PUT',body:JSON.stringify({mode:sel.value})});refresh()}},
   row:(it,left,right,refresh)=>{const cur=(it.extra||{}).current;
     left.innerHTML=`<img src="/api/wallpapers/${encodeURIComponent(it.name)}" alt="" style="height:3.4em;border-radius:.3em;border:1px solid var(--line);margin-right:.6em;vertical-align:middle">${it.name}`;
     right.insertAdjacentHTML('beforeend',`<span>${fmtB(it.bytes)}</span>`+(cur?'<span class="badge on">当前</span>':''));
     if(!cur){const b=document.createElement('button');b.className='btn';b.textContent='使用';b.onclick=async()=>{await j('/api/wallpapers/current',{method:'PUT',body:JSON.stringify({name:it.name})});refresh()};right.appendChild(b);
       right.appendChild(delBtn('删除 '+it.name+'？','/api/wallpapers/'+encodeURIComponent(it.name),refresh))}}})}}
};

/* 资产页模板（字体 / 壁纸）：说明 + 可选头部 + 上传区 + 列表。o: {title?,hint,header?,icon,label,accept,btn,listTitle?,onRender?(sec,refresh,data),row(it,left,right,refresh)} */
function assetTab(sec,api,o){sec.innerHTML=`<div class="card">${o.title?`<h2>${o.title}</h2>`:''}<p class="${o.title?'small':'lead'}">${o.hint}</p>${o.header||''}
  ${upHtml(o.icon,o.label,o.accept,o.btn)}
  <h3>${o.listTitle||'已安装'}</h3><ul class="list" id="al"></ul></div>`;
  const refresh=async()=>{const d=await j(api);fillList($('#al',sec),d.items||[],(it,left,right)=>o.row(it,left,right,refresh));if(o.onRender)o.onRender(sec,refresh,d)};
  uploader($('.up',sec),()=>api,()=>({}),o.accept,refresh);
  refresh();sec.refresh=refresh}

/* 「笔记」tab（note-serve 注册；数据来自 ink-serve 条目库）：按书→按章列条目，左裁图右文本，改即存。
   设备只负责写、不负责改：这里就是"改"的地方（e-ink 上打字太痛苦）。分区只管笔记本排版分组，不再驱动 AI；
   问 AI 改按条目单发——勾「问AI」+ 填问题+点提问，调 mind-serve 拼"书名+章节+勾画原文+转写文本+问题"发模型（二期，白皮书 §03n）。 */
const STYLE_NAMES={body:'正文',bullet:'无序 -',numbered:'有序 1.',checkbox:'待办 口'};
const STATUS_NAMES={mined:'待浏览',pending:'待转写',draft:'待校对',reviewed:'已校对',skipped:'已跳过',revoked:'已撤销'};
/* 「浏览」（新批注先落这，点了才转笔记）与「整理」（真被要求转笔记的才在这核对）拆两个子视图，见二期设计（白皮书 §03n）。 */
function renderNotes(sec){sec.innerHTML=`
  <div class="card"><h2>笔记</h2>
    <p class="lead">荧光笔勾书、在旁边手写，合上书先到「浏览」：看一眼，点「转入笔记」才会转写、进「整理」核对；点「不需要」就跳过，不再出现。</p>
    <div class="row"><span class="small">书</span><select id="nbook" style="flex:1;min-width:10em"></select><button class="btn" id="nrescan" title="清掉页记录，整本重新摄取">重扫</button></div>
    <div class="row small" id="nsum"></div>
  </div>
  <div class="subnav"><button class="on">👀 浏览</button><button>✎ 整理</button></div>
  <div class="subpanel on" id="nbrowse"></div>
  <div class="subpanel" id="norganize">
    <div class="card">
      <details class="cmp" id="nsecs"><summary>分区（笔记本排版分组；不再驱动 AI，问 AI 见每条下面）</summary><div id="nseclist"></div><button class="btn" id="nsecadd">＋ 分区</button> <button class="btn pri" id="nsecsave">保存分区</button></details>
      <details class="cmp" id="nmodels"><summary>模型（视觉转写 + 文字问答，各自 key/模型/用量）</summary>
        <div class="row" style="align-items:flex-start;gap:1.2em;flex-wrap:wrap">
          <div style="flex:1;min-width:14em">
            <div class="small" style="font-weight:600">👁 视觉（转写批注）</div>
            <div class="small" id="mvstat"></div>
            <div class="row"><input type="password" id="mvkey" placeholder="key（只写不回显；DashScope 百炼）" style="flex:1;min-width:10em" autocomplete="off"><button class="btn pri" id="mvsave">保存</button><button class="btn" id="mvclear" title="清掉已存的 key">清 key</button></div>
            <div class="row"><input type="text" id="mvmodel" placeholder="模型" style="max-width:9em"><input type="text" id="mvurl" placeholder="OpenAI 兼容口 baseUrl" style="flex:1;min-width:10em"></div>
          </div>
          <div style="flex:1;min-width:14em">
            <div class="small" style="font-weight:600">✎ 文字（问AI）</div>
            <div class="small" id="mtstat"></div>
            <div class="row"><input type="password" id="mtkey" placeholder="key（只写不回显；DashScope 百炼）" style="flex:1;min-width:10em" autocomplete="off"><button class="btn pri" id="mtsave">保存</button><button class="btn" id="mtclear" title="清掉已存的 key">清 key</button></div>
            <div class="row"><input type="text" id="mtmodel" placeholder="模型" style="max-width:9em"><input type="text" id="mturl" placeholder="OpenAI 兼容口 baseUrl" style="flex:1;min-width:10em"></div>
          </div>
        </div>
      </details>
      <details class="cmp" id="ntr"><summary>转写 <span class="small" id="ntrsum"></span></summary>
        <div class="row small" id="ntrstat"></div>
        <div class="row"><label class="toggle"><input type="checkbox" id="ntrauto"> 合书自动转写</label></div>
        <div class="row"><button class="btn" id="ntrrun">转写待转写条目</button><button class="btn" id="ntrretry" title="清掉失败记录再跑">重试失败</button><span class="small" id="ntrmsg"></span></div>
        <div class="small" id="ntrfail"></div>
      </details>
    </div>
    <div id="nchapters"></div>
  </div>`;
  const sel=$('#nbook',sec),chaps=$('#nchapters',sec),browse=$('#nbrowse',sec),sum=$('#nsum',sec);let book=null;
  const cropUrl=(uuid,f)=>`/api/ink/books/${encodeURIComponent(uuid)}/crops/${encodeURIComponent(f)}`;
  const patch=async(id,body)=>{const r=await postJ(`/api/ink/books/${encodeURIComponent(book.uuid)}/entries/${encodeURIComponent(id)}`,body);if(r.ok===false)alert(r.message||'保存失败')};
  /* 浏览态动作：Mined→Pending（转入笔记）/ Mined→Skipped（不需要），见 ink-serve::triage。成功后两个子视图都要重画（条目跨视图搬家）。 */
  const triage=async(id,action)=>{const r=await postJ(`/api/ink/books/${encodeURIComponent(book.uuid)}/entries/${encodeURIComponent(id)}/${action}`,{});if(r.ok===false)return;
    book=await j(`/api/ink/books/${encodeURIComponent(book.uuid)}`);renderBrowse();renderBook()};
  const updateSummary=()=>{if(!book){sum.textContent='';return}const es=book.entries||[];
    const c=st=>es.filter(e=>e.status===st).length,skipped=c('skipped');
    sum.textContent=`待浏览 ${c('mined')} · 待转写 ${c('pending')} · 待校对 ${c('draft')} · 已校对 ${c('reviewed')}${skipped?` · 已跳过 ${skipped}（不再显示）`:''}`};
  const renderSections=()=>{const el=$('#nseclist',sec);el.innerHTML='';(book.sections||[]).forEach((s,i)=>{const d=document.createElement('div');d.className='row';d.innerHTML=`<input type="text" value="${s.name}" placeholder="名字" style="max-width:6em" data-k="name"><input type="text" value="${s.brief||''}" placeholder="说明（可选，会印进笔记本这个分区的标题里）" style="flex:1;min-width:10em" data-k="brief"><button class="btn" title="删">✕</button>`;
    d.querySelectorAll('[data-k]').forEach(inp=>inp.onchange=()=>{book.sections[i][inp.dataset.k]=inp.type==='checkbox'?inp.checked:inp.value});d.querySelector('button').onclick=()=>{book.sections.splice(i,1);renderSections()};el.appendChild(d)})};
  $('#nsecadd',sec).onclick=()=>{book.sections.push({id:'s'+Date.now(),name:'',brief:'',ai:true,order:book.sections.length,triggers:[]});renderSections()};
  $('#nsecsave',sec).onclick=async()=>{const r=await j(`/api/ink/books/${encodeURIComponent(book.uuid)}/sections`,{method:'PUT',body:JSON.stringify({sections:book.sections})});if(r.ok===false)alert(r.message);else renderBook()};
  $('#nrescan',sec).onclick=async()=>{if(!book)return;await postJ(`/api/ink/books/${encodeURIComponent(book.uuid)}/rescan`,{});refresh()};
  /* 浏览：按页分组、只列 Mined（待决定的），最近变更的页在前；转入笔记/不需要两个按钮直接调 triage。 */
  const renderBrowse=()=>{browse.innerHTML='';if(!book)return;updateSummary();
    const mined=(book.entries||[]).filter(e=>e.status==='mined');
    if(!mined.length){browse.innerHTML='<div class="card"><p class="small">没有待浏览的批注——勾画/手写后合上书，稍等抓取即可出现在这里。</p></div>';return}
    const groups=new Map();mined.forEach(e=>{const k=e.page_index;if(!groups.has(k))groups.set(k,[]);groups.get(k).push(e)});
    const recency=k=>Math.max(...groups.get(k).map(e=>e.updated));
    [...groups.keys()].sort((a,b)=>recency(b)-recency(a)).forEach(k=>{const es=groups.get(k).sort((a,b)=>(a.ink?a.ink.bbox[1]:0)-(b.ink?b.ink.bbox[1]:0));
      const card=document.createElement('div');card.className='card';card.innerHTML=`<h3 style="margin-top:0">第 ${k+1} 页${es[0].chapter_title?' · '+es[0].chapter_title:''} <span class="small">${es.length} 条</span></h3>`;
      es.forEach(e=>{const row=document.createElement('div');row.className='opt-note';row.style.cssText='display:flex;gap:.6em;flex-wrap:wrap;align-items:flex-start;margin:.4em 0';
        const img=e.ink&&e.ink.crop?`<img src="${cropUrl(book.uuid,e.ink.crop)}" alt="手写" style="max-width:40%;max-height:9em;border:1px solid var(--line);background:#fff">`:'<span class="small">（无裁图）</span>';
        row.innerHTML=`${img}<div style="flex:1;min-width:12em">
          ${e.quote?`<div class="small" style="border-left:3px solid var(--line);padding-left:.5em;margin:.2em 0">「${e.quote.text}」</div>`:''}
          <div class="row"><button class="btn pri" data-a="request">转入笔记</button><button class="btn" data-a="skip">不需要</button></div></div>`;
        row.querySelector('[data-a="request"]').onclick=()=>triage(e.id,'request');
        row.querySelector('[data-a="skip"]').onclick=()=>triage(e.id,'skip');
        card.appendChild(row)});
      browse.appendChild(card)})};
  /* 整理：只列真被要求转笔记的（Pending/Draft/Reviewed）——Mined 在「浏览」决定，Skipped/Revoked 不再出现。 */
  const renderBook=()=>{chaps.innerHTML='';if(!book)return;renderSections();updateSummary();
    const live=(book.entries||[]).filter(e=>['pending','draft','reviewed'].includes(e.status));
    const groups=new Map();live.forEach(e=>{const k=e.chapter==null?-1:e.chapter;if(!groups.has(k))groups.set(k,[]);groups.get(k).push(e)});
    [...groups.keys()].sort((a,b)=>a-b).forEach(k=>{const es=groups.get(k).sort((a,b)=>a.page_index-b.page_index||(a.ink?a.ink.bbox[1]:0)-(b.ink?b.ink.bbox[1]:0));
      const card=document.createElement('div');card.className='card';card.innerHTML=`<h3 style="margin-top:0">${k<0?'（未归章）':`第 ${k+1} 章 · ${es[0].chapter_title||''}`} <span class="small">${es.length} 条</span></h3>`;
      es.forEach(e=>{const row=document.createElement('div');row.className='opt-note';row.style.cssText='display:flex;gap:.6em;flex-wrap:wrap;align-items:flex-start;margin:.4em 0';
        const img=e.ink&&e.ink.crop?`<img src="${cropUrl(book.uuid,e.ink.crop)}" alt="手写" style="max-width:40%;max-height:9em;border:1px solid var(--line);background:#fff">`:'<span class="small">（无裁图）</span>';
        const opts=(book.sections||[]).map(s=>`<option value="${s.id}" ${e.section===s.id?'selected':''}>${s.name}</option>`).join('');
        const sty=Object.entries(STYLE_NAMES).map(([v,n])=>`<option value="${v}" ${e.style===v?'selected':''}>${n}</option>`).join('');
        const draft=(e.drafts&&e.drafts[0])?e.drafts[0].text:'';
        row.innerHTML=`${img}<div style="flex:1;min-width:12em">
          <div class="small">p.${e.page_index+1}${e.subhead?' · '+e.subhead:''} <span class="badge ${e.status==='reviewed'?'on':''}">${STATUS_NAMES[e.status]||e.status}</span></div>
          ${e.quote?`<div class="small" style="border-left:3px solid var(--line);padding-left:.5em;margin:.2em 0">「${e.quote.text}」</div>`:''}
          <textarea rows="2" style="width:100%;box-sizing:border-box" placeholder="${draft?'转写：'+draft:'等待转写…'}">${e.text||draft}</textarea>
          <div class="row"><select data-k="section"><option value="">（未分区）</option>${opts}</select><select data-k="style">${sty}</select></div>
          <div class="row" style="margin-top:.3em"><label class="toggle"><input type="checkbox" data-ask ${e.ask_ai?'checked':''}> 问AI</label>
            <input type="text" data-question placeholder="问题…（如「他是谁」）" value="${e.question?e.question.replace(/"/g,'&quot;'):''}" style="flex:1;min-width:9em" ${e.ask_ai?'':'disabled'}>
            <button class="btn" data-askbtn ${e.ask_ai&&e.question?'':'disabled'}>提问</button></div>
          ${e.answer?`<details class="cmp" open><summary>回答 <span class="small">「${e.answer.brief}」</span></summary><div class="small">${e.answer.text}</div></details>`:''}</div>`;
        row.querySelector('textarea').onchange=ev=>patch(e.id,{text:ev.target.value});
        row.querySelectorAll('select').forEach(s=>s.onchange=()=>patch(e.id,{[s.dataset.k]:s.value}));
        if(e.ink&&e.ink.crop){const rb=document.createElement('button');rb.className='btn';rb.textContent=draft?'重转':'转写';rb.title='用当前后端转写这一条（不动已校对文本）';rb.style.marginLeft='.4em';
          rb.onclick=async()=>{rb.disabled=true;const r=await j(`/api/transcribe/books/${encodeURIComponent(book.uuid)}/entries/${encodeURIComponent(e.id)}`,{method:'POST'});rb.disabled=false;if(r.ok===false)alert(r.message||'转写失败')};row.querySelector('.row').appendChild(rb)}
        /* 「问AI」勾选框 + 问题 + 提问按钮：改即存（ink-serve），点提问才真的调 mind-serve。 */
        const askBox=row.querySelector('[data-ask]'),qInput=row.querySelector('[data-question]'),askBtn=row.querySelector('[data-askbtn]');
        const syncAskUi=()=>{qInput.disabled=!askBox.checked;askBtn.disabled=!(askBox.checked&&qInput.value.trim())};
        askBox.onchange=()=>{patch(e.id,{askAi:askBox.checked});syncAskUi()};
        qInput.onchange=()=>{patch(e.id,{question:qInput.value});syncAskUi()};
        askBtn.onclick=async()=>{askBtn.disabled=true;askBtn.textContent='提问中…';
          const r=await j(`/api/mind/books/${encodeURIComponent(book.uuid)}/entries/${encodeURIComponent(e.id)}/ask`,{method:'POST'});
          askBtn.disabled=false;askBtn.textContent='提问';
          if(r.ok===false)alert(r.message||'提问失败');else{book=await j(`/api/ink/books/${encodeURIComponent(book.uuid)}`);renderBook()}};
        card.appendChild(row)});
      chaps.appendChild(card)})};
  const loadBook=async()=>{if(!sel.value){book=null;renderBrowse();renderBook();return}book=await j(`/api/ink/books/${encodeURIComponent(sel.value)}`);renderBrowse();renderBook()};
  sel.onchange=loadBook;
  /* 转写区（transcribe-serve）：状态 = 待转写数 / 用量 / 最近一轮 / 失败清单；key/模型改到「模型」统一面板 */
  const trMsg=$('#ntrmsg',sec);
  const trRefresh=async()=>{const st=await j('/api/transcribe/status');const box=$('#ntr',sec);if(st.ok===false){box.style.display='none';return}box.style.display='';
    const c=st.config||{},u=st.usage||{},lr=u.lastRun;
    $('#ntrsum',sec).textContent=`${c.hasKey?'key ✓':'未配 key'} · 待转写 ${st.pending||0}`;
    $('#ntrstat',sec).textContent=`${c.backend||''} ${c.model||''} · 累计 ${u.ok||0} 成 ${u.failed||0} 败 · token 入 ${u.promptTokens||0} 出 ${u.completionTokens||0}${lr?` · 上轮 扫 ${lr.scanned} 成 ${lr.done} 败 ${lr.failed}${lr.note?' · '+lr.note:''}`:''}${u.lastError?' · 最近错误：'+u.lastError:''}`;
    $('#ntrauto',sec).checked=!!c.auto;
    $('#ntrfail',sec).innerHTML=(st.failures||[]).length?'失败：'+st.failures.map(f=>`${f.id.slice(0,8)}… ×${f.attempts} ${f.error}`).join('；'):''};
  $('#ntrauto',sec).onchange=async()=>{const r=await j('/api/transcribe/config',{method:'PUT',body:JSON.stringify({auto:$('#ntrauto',sec).checked})});if(r.ok===false)alert(r.message||'保存失败');trRefresh()};
  const trRun=async(url)=>{trMsg.textContent='转写中…';const r=await j(url,{method:'POST'});trMsg.textContent=r.ok===false?('✗ '+(r.message||'失败')):(r.done!=null?`✓ 成 ${r.done} 败 ${r.failed} 余 ${r.left}${r.note?' · '+r.note:''}`:'✓');trRefresh()};
  $('#ntrrun',sec).onclick=()=>trRun('/api/transcribe/run');
  $('#ntrretry',sec).onclick=()=>trRun('/api/transcribe/retry');
  /* 「模型」统一面板（二期步骤 5）：视觉（transcribe）+ 文字（mind）并排，各自 key（脱敏预览+保存/清）+ 模型/baseUrl + 用量。 */
  const mFill=(prefix,st)=>{
    if(st.ok===false){$(`#${prefix}stat`,sec).textContent='服务未就绪：'+(st.message||'');return}
    const c=st.config||{},u=st.usage||{};
    $(`#${prefix}stat`,sec).textContent=`${c.keyMasked?'key '+c.keyMasked:(c.hasKey?'key ✓（环境变量）':'未配 key')} · 调用 ${u.calls||0}（成 ${u.ok||0} 败 ${u.failed||0}）· token 入 ${u.promptTokens||0} 出 ${u.completionTokens||0}${u.lastError?' · 最近错误：'+u.lastError:''}`;
    $(`#${prefix}model`,sec).value=c.model||'';$(`#${prefix}url`,sec).value=c.baseUrl||''};
  const mRefresh=async()=>{const [vt,tt]=await Promise.all([j('/api/transcribe/status'),j('/api/mind/status')]);mFill('mv',vt);mFill('mt',tt)};
  const mSave=async(seg,prefix)=>{
    const key=$(`#${prefix}key`,sec).value,model=$(`#${prefix}model`,sec).value,baseUrl=$(`#${prefix}url`,sec).value;
    const r=await j(`/api/${seg}/config`,{method:'PUT',body:JSON.stringify({apiKey:key,model,baseUrl})});
    if(r.ok===false)alert(r.message||'保存失败');else $(`#${prefix}key`,sec).value='';
    mRefresh()};
  const mClear=async(seg,label)=>{if(!confirm(`清掉${label}已存的 API key？`))return;const r=await j(`/api/${seg}/config`,{method:'PUT',body:JSON.stringify({clearKey:true})});if(r.ok===false)alert(r.message||'清除失败');mRefresh()};
  $('#mvsave',sec).onclick=()=>mSave('transcribe','mv');
  $('#mtsave',sec).onclick=()=>mSave('mind','mt');
  $('#mvclear',sec).onclick=()=>mClear('transcribe','视觉模型');
  $('#mtclear',sec).onclick=()=>mClear('mind','文字模型');
  const refresh=async()=>{const d=await j('/api/ink/books');const cur=sel.value;sel.innerHTML=(d.items||[]).map(b=>`<option value="${b.uuid}">${b.title}（${b.entries}）</option>`).join('')||'<option value="">（还没有勾画过的书）</option>';
    if(cur&&[...sel.options].some(o=>o.value===cur))sel.value=cur;await Promise.all([loadBook(),trRefresh(),mRefresh()])};
  refresh();sec.refresh=refresh;subtabs(sec)}

/* 管理台/引导（固定 tab，始终在——它是网关自身页面，不由服务注册表驱动） */
function renderManage(sec){sec.innerHTML=`
  <div class="card"><h2>引导 · 基石</h2><p class="lead">书架的功能建在 xovi + appload 之上。先用桌面端 <b>reManager</b>（或设备上的 vellum）把基石装好，KOReader 走官方仓库自装，再回这里管理书架各功能。</p>
    <div class="kv small" id="found">检测中…</div>
    <p class="small">下载 / 文档：<a href="https://github.com/rmitchellscott/reManager" target="_blank" rel="noopener">reManager</a>（桌面端 · vellum 生态）· <a href="https://github.com/asivery/rmpp-xovi" target="_blank" rel="noopener">xovi</a> · <a href="https://github.com/koreader/koreader/wiki" target="_blank" rel="noopener">KOReader Wiki</a></p></div>
  <div class="card"><h2>电脑端 · <code>shelf push</code>（进阶洗书 / PDF 重排）</h2>
    <p class="lead">难搞的书用它：非标准格式转 EPUB、Calibre 级深洗、PDF 论文重排、漫画转 CBZ——设备端做不到的都在这。</p>
    <div class="opt-note">
      <b>命令长这样</b>（在本仓库目录下跑；<code>shelf/host/bin/shelf</code> 就是那个命令，嫌长可 <code>alias shelf="$PWD/shelf/host/bin/shelf"</code>）：<br>
      <code>shelf/host/bin/shelf push &lt;书1&gt; [书2 …]</code>
      <div class="small" style="margin-top:.4em">
        · 后面只跟<b>要投的书</b>（可一次多本）；<b>没有输出路径、也没有目标参数</b>——洗完一律落到<b>母版库</b>，放哪个读器你在网页「传书 → 母版库」里点。<br>
        · 有 Calibre 就先洗（EPUB 深洗 / 杂格式转 EPUB / PDF 结构化重排，&gt;60MB 的 PDF 自动分卷）；漫画自动识别转 CBZ（<code>--comic/--no-comic</code> 覆盖）；<code>--no-optimize</code> 不洗原样传；<code>--to-pdf</code> 定稿成手写批注用的 PDF。<br>
        · 和网页规则一致：<b>所有书只落母版库</b>，没有直投读器的选项。
      </div>
    </div>
    <details class="cmp"><summary>例子 / 强在哪 / 怎么装</summary>
    <dl class="help">
      <dt>例子</dt>
      <dd><code>shelf/host/bin/shelf push 论文.pdf</code> — PDF 结构化重排 → 母版库，再到「传书 → 母版库」选去向<br>
          <code>shelf/host/bin/shelf push 小说.azw3</code> — 转干净 EPUB → 母版库（两个读器都能去）<br>
          <code>shelf/host/bin/shelf push 漫画.azw3</code> — 自动识别漫画 → CBZ → 母版库（点「加入 KOReader」；漫画不投原生）<br>
          <code>shelf/host/bin/shelf push 书.epub --to-pdf</code> — 定稿固定版式 PDF → 母版库（投 xochitl 手写批注）<br>
          <code>shelf/host/bin/shelf push a.epub b.mobi</code> — 一次多本<br>
          <code>shelf/host/bin/shelf status</code> · <code>doctor</code> — 看设备连通 / 环境</dd>
      <dt>强在哪</dt>
      <dd>① <b>杂格式转干净 EPUB 进原生</b>：AZW3 / MOBI / AZW / PRC / FB2 直接上传只能进 KOReader，这里能转成 EPUB 投原生；② <b>Calibre 级深洗</b>：CSS 拍平比端上更彻底，排版锁死的书也能救；③ <b>PDF 论文重排</b>：多列 / 公式 / 图按阅读顺序重排到屏宽——<b>端上做不到</b>（端上 PDF 只原样直传）；④ 扫描件走 k2pdfopt。产物再叠加设备同款优化器，观感与网页直传一致。</dd>
      <dt>怎么装</dt>
      <dd>电脑装 <a href="https://calibre-ebook.com/" target="_blank" rel="noopener">Calibre</a>（含 ebook-convert）+ Python 3 → 克隆本仓库 → <code>cd shelf &amp;&amp; sh build.sh</code>（编出 <code>epub-optimize</code>）→ 就能用 <code>shelf/host/bin/shelf</code> 了。</dd>
    </dl></details></div>
  <div class="card"><h2>书架功能</h2>
    <p class="lead">每个功能可单独<b>开关</b>、<b>卸载</b>；未装的按命令安装。网关（本页）始终在。</p>
    <details class="cmp"><summary>三态 / 开关 / 卸载 / 安装 是什么？（点开看说明）</summary>
      <dl class="help">
        <dt>三种状态</dt>
        <dd><span class="badge off">未装</span> 设备上没这个程序 → 按给出的命令安装。<br>
            <span class="badge">已装·未开</span> 程序在、后台没跑 → 网页看不到它的功能，点「开启」启用。<br>
            <span class="badge on">已开</span> 后台在跑 → 顶部有它的标签页，功能可用。</dd>
        <dt>开启 / 关闭</dt>
        <dd><b>关闭＝只停后台服务</b>：网页隐藏该标签，但<b>已经生效的东西照常用</b>——已装字体仍能在阅读器里选、壁纸仍显示、KOReader 仍能打开；只是不能再用网页传 / 改它。用途是隐藏用不到的功能、减少对外暴露面。</dd>
        <dt>常开会不会卡 / 费电？</dt>
        <dd>不会。实测 5 个服务全部常开共约 <b>9 MB 内存</b>、开机一整天累计不到 <b>2 秒 CPU</b>（平均约 0.002%），平时都阻塞在等请求、不抢 CPU。<b>对看书 / 记笔记零可感影响，不卡、不额外费电。</b>建议全部常开，除非某功能你确定永远不用。</dd>
        <dt>卸载</dt>
        <dd>删掉该功能的程序、systemd 单元和相关注入文件（qmd）。<b>你传过的书 / 字体 / 壁纸等用户数据保留。</b>卸载后它从网页消失；想再用按安装命令重装。网关不能从网页关或卸——它是本管理页的宿主。</dd>
        <dt>安装为什么不在网页做？</dt>
        <dd>安装要重挂载只读系统分区、写系统单元，风险偏高。<b>未装功能只给命令</b>：在电脑上 SSH 跑，或走 reManager 引导，更安全。</dd>
      </dl></details>
    <div class="row"><button class="btn" id="allon">全部开启</button><button class="btn" id="alloff">全部关闭（留网关）</button></div>
    <ul class="list" id="mods"></ul></div>`;
  const badge=(t,ok)=>`<span class="badge ${ok?'on':'off'}">${t}</span>`;
  const refresh=async()=>{
    const f=await j('/api/foundation');$('#found',sec).innerHTML=f.ok===false?`<span>${f.message}</span>`:
      `<b>xovi</b><span>${badge(f.xovi?'已装':'未装',f.xovi)}</span><b>appload</b><span>${badge(f.appload?'已装':'未装',f.appload)}</span><b>qt-resource-rebuilder</b><span>${badge(f.qrr?'已装':'未装',f.qrr)}</span><b>KOReader</b><span>${badge(f.koreader?'已装':'未装',f.koreader)}</span>`;
    const d=await j('/api/manage');const ul=$('#mods',sec);ul.innerHTML='';(d.modules||[]).forEach(m=>{const li=document.createElement('li');li.style.flexWrap='wrap';
      let state,cls;if(!m.installable){state='未上线';cls=''}else if(!m.installed){state='未装';cls='off'}else if(m.running){state='已开';cls='on'}else{state='已装·未开';cls=''}
      const left=document.createElement('span');left.innerHTML=`${m.label} <span class="small">${m.service}</span> <span class="badge ${cls}">${state}</span>`;
      const right=document.createElement('span');right.style.cssText='display:flex;gap:.4em;align-items:center';
      if(m.installable&&m.installed){
        const t=document.createElement('button');t.className='btn';t.textContent=m.running?'关闭':'开启';
        t.onclick=async()=>{const r=await j('/api/manage/'+m.seg+'/'+(m.running?'stop':'start'),{method:'POST'});if(r.ok===false)alert(r.message);setTimeout(refresh,600)};right.appendChild(t);
        const u=document.createElement('button');u.className='btn';u.textContent='卸载';
        u.onclick=async()=>{if(confirm('卸载 '+m.label+'？删除它的服务/单元/相关 qmd（用户数据保留）。')){const r=await j('/api/manage/'+m.seg+'/uninstall',{method:'POST'});if(r.ok===false)alert(r.message);else alert('已卸载 '+m.label);setTimeout(()=>location.reload(),800)}};right.appendChild(u);
      }else if(m.installable){const g=document.createElement('span');g.className='small';g.innerHTML='装：<code>shelf/install.sh --only '+m.only+'</code>';right.appendChild(g)}
      li.append(left,right);ul.appendChild(li)});};
  $('#allon',sec).onclick=async()=>{const d=await j('/api/manage');for(const m of (d.modules||[]))if(m.installable&&m.installed&&!m.running)await j('/api/manage/'+m.seg+'/start',{method:'POST'});refresh()};
  $('#alloff',sec).onclick=async()=>{if(!confirm('关闭全部领域服务（网关保留）？'))return;const d=await j('/api/manage');for(const m of (d.modules||[]))if(m.installable&&m.installed&&m.running)await j('/api/manage/'+m.seg+'/stop',{method:'POST'});refresh()};
  refresh();sec.refresh=refresh;}

(async()=>{const d=await j('/api/services');const svcs=(d.services||[]).filter(s=>s.ui&&TABS[s.name]).sort((a,b)=>a.ui.order-b.ui.order);
  $('#hdr').textContent=location.host;
  const nav=$('#tabs'),main=$('#main');main.innerHTML='';
  const secByArea={};const dirty=new Set();
  const addTab=(title,render,first,area)=>{const b=document.createElement('button');b.textContent=title;const sec=document.createElement('section');sec.area=area;secByArea[area]=sec;
    b.onclick=()=>{[...nav.children].forEach(x=>x.classList.remove('on'));[...main.children].forEach(x=>x.classList.remove('on'));b.classList.add('on');sec.classList.add('on');dirty.delete(area);if(sec.refresh)sec.refresh()};
    nav.appendChild(b);main.appendChild(sec);render(sec);if(first)b.onclick()};
  addTab('传书',renderTransfer,true,'books');          // 总入口（入库｜母版库），固定第一位（book-serve 不在时列表里提示去管理页开）
  svcs.forEach((s)=>addTab(TABS[s.name].title,TABS[s.name].render,false,AREA[s.name]||s.name));
  addTab('管理',renderManage,false,'manage');            // 固定管理台，始终可进
  /* 事件推送（SSE，零轮询）：服务在变更处发事件 → 网关 /api/events 汇聚 → 这里只刷对应 tab；不在前台的 tab 记脏，切过去时刷。
     manage 事件（服务启停）：tab 集合变了就整页重载，否则只刷管理台。断线（WiFi 掉/设备休眠醒来）EventSource 自动重连。 */
  const svcKey=svcs.map(s=>s.name).join(',');
  const dot=document.createElement('span');dot.id='live';dot.title='事件推送';dot.textContent='●';dot.style.cssText='margin-left:.5em;font-size:.8em;color:var(--bad)';$('#hdr').appendChild(dot);
  const es=new EventSource('/api/events');
  es.onopen=()=>{dot.style.color='var(--ok)';dot.title='事件推送已连接'};
  es.onerror=()=>{dot.style.color='var(--bad)';dot.title='事件推送断开，自动重连中'};
  es.onmessage=async(e)=>{let ev;try{ev=JSON.parse(e.data)}catch{return}
    if(ev.area==='manage'){const d=await j('/api/services');const k=(d.services||[]).filter(s=>s.ui&&TABS[s.name]).map(s=>s.name).join(',');if(k!==svcKey){location.reload();return}}
    const sec=secByArea[ev.area];if(!sec)return;
    if(sec.classList.contains('on')){if(sec.refresh)sec.refresh()}else dirty.add(ev.area)};
})();
