"""子命令注册表（Command 模式）：每个模块暴露 `NAME`、`HELP`、`add_args(parser)`、`run(args, ctx) -> int`。"""
from . import doctor, push, services, status

ALL = [push, services, status, doctor]
