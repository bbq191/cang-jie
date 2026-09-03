NAME = "status"
HELP = "网关与各服务健康状态"


def add_args(p):
    pass


def run(args, ctx) -> int:
    ok = True
    try:
        svcs = ctx.transport.get("/api/services").get("services", [])
    except Exception as e:  # noqa: BLE001
        print(f"网关 {ctx.config.base_url}: 不可达（{e}）")
        return 1
    print(f"网关 {ctx.config.base_url}: 在线")
    seg = {"book-serve": "books", "koreader-serve": "koreader", "font-serve": "fonts", "wallpaper-serve": "wallpapers", "weread-serve": "weread"}
    for s in svcs:
        if s["name"] not in seg:
            continue
        try:
            h = ctx.transport.get(f"/api/{seg[s['name']]}/health")
            print(f"  {s['name']:<16} {'ok' if h.get('ok') else 'bad'}  v{h.get('version', '?')}")
        except Exception as e:  # noqa: BLE001
            ok = False
            print(f"  {s['name']:<16} 异常：{e}")
    return 0 if ok else 1
