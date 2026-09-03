//! 单页 UI 占位（Phase 1 换成完整三 tab 页面）：列出活着的服务。
pub const PAGE: &str = r#"<!doctype html><html lang="zh"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>书架</title><style>body{font-family:system-ui,sans-serif;margin:2em;max-width:40em}li{margin:.4em 0}</style></head>
<body><h1>书架</h1><p>已安装的服务：</p><ul id="s"><li>加载中…</li></ul>
<script>fetch('/api/services').then(r=>r.json()).then(j=>{const u=document.getElementById('s');u.innerHTML='';
for(const s of j.services){const li=document.createElement('li');li.textContent=s.label+' ('+s.name+' v'+s.version+')';u.appendChild(li);}
if(!j.services.length)u.innerHTML='<li>（无）</li>';}).catch(()=>{});</script></body></html>"#;
