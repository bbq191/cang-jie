/* hw-stroke —— CJK 手写笔迹渲染优化。
 *
 * 第一轮真机实验（factor 统一缩放）验证过"改变宽笔画几何生成函数的入口宽度，
 * 能不能真的影响渲染出来的笔迹粗细"这件事本身可行（真机截图肉眼确认）。
 *
 * 第二轮：笔尖角度模型（西式书法笔工具的经典公式，社区调研见白皮书 §03f）：
 *   宽度 ×= min_ratio + (1-min_ratio) × |sin(运笔方向角 − 笔尖固定角度)|
 * 运笔方向角**不依赖点结构里的方向字节**（拿真机诊断 hook 实测过：日常用的
 * "中粗钢笔"走的是 `bVar16<4` 那条纯线性分支，压根不会往点结构方向字段那条
 * 路径走）——改成在 `FUN_00f47530` 自己内部本来就维护的"上一个点坐标"
 * （`ctx+0x48`/`ctx+0x4c`）现算 `atan2(dy,dx)`，这段状态是这个函数自己的、
 * 所有笔型都统一走到，不挑分支。
 *
 * hook 目标 `FUN_00f47530`（变宽笔画的几何生成器）是从 `ShapesOverlay::
 * updateImage`（`FUN_008bbb80`）反编译直接跟踪下来的，不是靠 vtable 成员关系
 * 猜的——`VaryingGenerator_WidthLength::generate()`（`FUN_00f401f0`）那次反编译
 * 简化过头、从未被真实调用点引用过，已经在 ../README.md 里勘误，这次不拿它当
 * hook 目标。
 *
 * 复用 chinese-ime/langhook 已经模块化出来的三个纯工具文件（跟 enhance/hl-snap
 * 同样的路径引用方式，见那边的头注）；patch_target/make_call_through_stub 逐
 * 字节抄自 enhance/hl-snap/src/hl_snap.c（这两个通用 trampoline 安装函数不是本
 * 次功能定制逻辑）。
 *
 * `FUN_00f47530(float x, float y, void *ctx)` 是标准 AAPCS64 调用约定
 * （两个 float 走 s0/s1，一个指针走 x0），handler 签名照抄这个约定，不需要
 * 处理特殊 ABI。函数入口第一件事就是 `*(float*)((char*)ctx+4) * 0.5` 算半宽，
 * `(char*)ctx+4` 就是"当前点宽度"，调用方（FUN_00f3f9d0）刚存进去、这个函数
 * 刚读出来就用——在这里改这个值最简单可靠。
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdint.h>
#include <stdbool.h>
#include <math.h>

#include "scan.h"
#include "pattern.h"
#include "trampoline_aarch64.h"

#define TARGET_MODULE_SUFFIX "/usr/bin/xochitl"
#define CJ_DATA_DIR "/home/root/.local/share/cangjie-ime"
#define CJ_READING_QOL_PATH CJ_DATA_DIR "/reading-qol.json"

/* FUN_00f47530 前 20 字节是纯栈/寄存器操作（paciasp/stp x29,x30/mov x29,sp/
 * stp x19,x20/mov x19,x0），没有分支、没有 PC 相对寻址，安全可 patch——但这
 * 20 字节本身太通用（拿真机 3.28.0.172 二进制实测，这个形状在全文件里命中
 * 113 次！常见 C++ 方法序言长这样的太多了），不能只拿这 20 字节当特征码。
 * 签名延长到 32 字节（再加 3 条指令：ldrb w0,[x0,#0x5a] / stp d12,d13,[sp,
 * #0x40] / fmov s13,s1，offset 0x5a 这种具体立即数足够把命中收窄到 1），真机
 * 实测确认唯一命中——签名比 PATCH_LEN 长没问题，patch_target 依然只覆盖前
 * PATCH_LEN(20) 字节，多出来的字节只用来提高定位唯一性，不参与 patch。 */
static const uint8_t PROLOGUE_HW_QUAD[] = {
    0x3f, 0x23, 0x03, 0xd5, 0xfd, 0x7b, 0xb9, 0xa9, 0xfd, 0x03, 0x00, 0x91,
    0xf3, 0x53, 0x01, 0xa9, 0xf3, 0x03, 0x00, 0xaa, 0x00, 0x68, 0x41, 0x39,
    0xec, 0x37, 0x04, 0x6d, 0x2d, 0x40, 0x20, 0x1e,
};

/* FUN_00f3f9d0 前 32 字节（paciasp/sub sp,#0x130/cmp w2,#1/stp x29,x30/add
 * x29,sp,#0x40/stp x19,x20/mov x19,x1/stp x21,x22），全是纯栈/寄存器操作，
 * 拿真机 3.28.0.172 二进制实测：12 字节起即唯一命中，这里用满 32 字节留
 * 余量。这是逐点渲染分派函数——诊断 hook 用，探查真机写字实际走哪个笔型
 * 分支，见 cj_hw_dispatch_handler 头注。 */
static const uint8_t PROLOGUE_HW_DISPATCH[] = {
    0x3f, 0x23, 0x03, 0xd5, 0xff, 0xc3, 0x04, 0xd1, 0x5f, 0x04, 0x00, 0x71,
    0xfd, 0x7b, 0x04, 0xa9, 0xfd, 0x03, 0x01, 0x91, 0xf3, 0x53, 0x05, 0xa9,
    0xf3, 0x03, 0x01, 0xaa, 0xf5, 0x5b, 0x06, 0xa9,
};

#define CJ_FAR_JUMP_LEN_LOCAL (5 * 4)
#define PATCH_LEN CJ_FAR_JUMP_LEN_LOCAL /* 覆盖目标函数开头的字节数，跟远跳转指令长度一致 */

/* ---- 通用 trampoline 安装（逐字节抄自 hl_snap.c，不做任何改动） ---- */

static void *make_call_through_stub(const uint8_t *original_bytes, void *jump_back_target) {
    size_t stub_len = PATCH_LEN + CJ_FAR_JUMP_LEN;
    void *stub = mmap(NULL, stub_len, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (stub == MAP_FAILED) return NULL;

    memcpy(stub, original_bytes, PATCH_LEN);

    uint32_t jump_instrs[5];
    cj_build_far_jump(jump_instrs, jump_back_target);
    memcpy((uint8_t *)stub + PATCH_LEN, jump_instrs, CJ_FAR_JUMP_LEN);

    if (mprotect(stub, stub_len, PROT_READ | PROT_EXEC) != 0) {
        munmap(stub, stub_len);
        return NULL;
    }
    __builtin___clear_cache((char *)stub, (char *)stub + stub_len);
    return stub;
}

static int patch_target(void *target_addr, void *handler, void **out_stub) {
    long pagesize = sysconf(_SC_PAGESIZE);
    if (pagesize <= 0) pagesize = 4096;

    uintptr_t page_base = (uintptr_t)target_addr & ~((uintptr_t)pagesize - 1);
    size_t region_len = (size_t)pagesize;
    if ((((uintptr_t)target_addr - page_base) + PATCH_LEN) > region_len) {
        region_len += (size_t)pagesize;
    }

    if (mprotect((void *)page_base, region_len, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
        fprintf(stderr, "[hw-stroke] mprotect 失败，放弃 hook（safe mode）：%s\n", strerror(errno));
        return 0;
    }

    void *jump_back_target = (uint8_t *)target_addr + PATCH_LEN;
    void *stub = make_call_through_stub((const uint8_t *)target_addr, jump_back_target);
    if (!stub) {
        fprintf(stderr, "[hw-stroke] 调用桩分配失败，放弃 hook（safe mode）\n");
        return 0;
    }
    *out_stub = stub;

    uint32_t jump_to_handler[5];
    cj_build_far_jump(jump_to_handler, handler);
    memcpy(target_addr, jump_to_handler, CJ_FAR_JUMP_LEN);
    __builtin___clear_cache((char *)target_addr, (char *)target_addr + CJ_FAR_JUMP_LEN);

    return 1;
}

/* ---- 宽度实验本体 ---- */

typedef void (*orig_hw_quad_fn_t)(float, float, void *);
static orig_hw_quad_fn_t g_orig_hw_quad_call_through = NULL;

/* 宽度缩放系数：1.0 = 不改变行为（默认，fail-safe）。第一轮真机验证先保持
 * 默认值确认"零写入、纯诊断"这一步的日志/数值符合预期，再手动改配置到
 * 2.0/0.5 验证"改这条渲染路径能真实影响笔迹粗细"这件事本身是否可行——两步
 * 验证用同一份构建，不用重新编译部署，改配置文件即可。 */
static float g_width_factor = 1.0f;

/* 笔尖角度模型（西式书法笔工具经典公式，社区调研见白皮书 §03f）：
 *   宽度 ×= min_ratio + (1-min_ratio) × |sin(运笔方向角 − 笔尖固定角度)|
 * min_ratio=1.0 时公式恒等于 1（不管方向角多少，乘数都是 1），等价于关闭——
 * 默认值，fail-safe。角度用角度制存、用的时候转弧度，方便手改配置文件。 */
static float g_nib_angle_deg = 45.0f;
static float g_nib_min_ratio = 1.0f;

/* 真机实测发现：钢笔（细笔画，w 大概 1.5~7.5）跟毛笔（粗笔画，w 能到 67）
 * 走的是完全同一条代码分支、同一套公式——用户反馈"钢笔上效果不好、毛笔上
 * 还行"，根因不是分支不同，是同一个比例摆动在细笔画上显得突兀、在粗笔画上
 * 才像书法笔触该有的过渡。不按笔型分支（两者本来就没区别），改成效果强度
 * 跟基础宽度挂钩——细笔画自动趋近"关闭"，粗笔画自动趋近完整强度。 */
static float g_nib_width_low = 6.0f;   /* w 低于这个值，效果趋近关闭 */
static float g_nib_width_high = 20.0f; /* w 高于这个值，效果满强度（g_nib_min_ratio） */

/* ⚠️ 曾经试过按笔型标签（`*(byte*)(lVar7+0x70)`，FUN_00f3f9d0 里的 bVar16）
 * 精确排除钢笔，撤回了——那个字段是 FUN_00f3f9d0 自己 `lVar7` 对象上的，
 * 钢笔走的 `bVar16<4` 分支用 `plVar6`（另一个指针）做虚函数调用，从没验证过
 * 这条路径下 FUN_00f47530 收到的 ctx 就是同一个 lVar7——真机实测大号
 * paintbrush 时这个假设直接崩了（读出来的"笔型"在 0~255 上随机跳，w 却是
 * 正常连续变化的真实数据），说明 ctx+0x70 对这条路径读的是无关内存，不是
 * 真的笔型标签。**只留纯宽度渐变**（ctx+4/0x48/0x4c/0x5a 是 FUN_00f47530
 * 自己定义并使用的字段，不管上游从哪条路径调进来，这几个偏移都可靠——
 * FUN_00f47530 自己的反编译代码直接用到它们，不是借别的函数的解读）。见
 * 白皮书 §03f「已知局限」。 */

/* 从已经读进内存的 reading-qol.json 内容里找一个浮点字段，找不到/解析失败
 * 不改 *out（调用方决定初始默认值）。仿 hl_snap.c 的极简字段扫描手法，不引
 * JSON 库——跟 cj_hw_refresh_config 里三个字段共用，避免三份重复的
 * strstr+strtod 样板。 */
static void cj_hw_read_float_key(const char *buf, const char *key, float *out) {
    const char *p = strstr(buf, key);
    if (!p) return;
    p += strlen(key);
    while (*p == ':' || *p == ' ' || *p == '\t' || *p == '"') p++;
    char *end = NULL;
    double v = strtod(p, &end);
    if (end == p) return; /* 没解析出数字，保持原值 */
    *out = (float)v;
}

/* 运行时开关：从 reading-qol.json 读 hwStrokeWidthFactor/hwStrokeNibAngleDeg/
 * hwStrokeNibMinRatio。fail-safe：文件缺失/字段缺失/解析失败 → 不改，保持
 * 当前值（初始默认=不改变行为）；解出来的值超出合理范围也拒绝，不写入。
 * 划一笔才调，非热路径，每次读一次即可。 */
static void cj_hw_refresh_config(void) {
    FILE *f = fopen(CJ_READING_QOL_PATH, "rb");
    if (!f) return;
    char buf[4096];
    size_t n = fread(buf, 1, sizeof(buf) - 1, f);
    fclose(f);
    buf[n] = '\0';

    float width_factor = g_width_factor;
    cj_hw_read_float_key(buf, "\"hwStrokeWidthFactor\"", &width_factor);
    if (width_factor > 0.0f && width_factor <= 10.0f) { /* ≤0 会让笔画消失，过大可能撑爆渲染/裁剪逻辑 */
        g_width_factor = width_factor;
    }

    float nib_angle = g_nib_angle_deg;
    cj_hw_read_float_key(buf, "\"hwStrokeNibAngleDeg\"", &nib_angle);
    if (nib_angle >= -360.0f && nib_angle <= 360.0f) {
        g_nib_angle_deg = nib_angle;
    }

    float nib_ratio = g_nib_min_ratio;
    cj_hw_read_float_key(buf, "\"hwStrokeNibMinRatio\"", &nib_ratio);
    if (nib_ratio >= 0.0f && nib_ratio <= 1.0f) { /* >1 或 <0 都没有物理意义（不是"最小比例"了） */
        g_nib_min_ratio = nib_ratio;
    }

    float width_low = g_nib_width_low;
    cj_hw_read_float_key(buf, "\"hwStrokeNibWidthLow\"", &width_low);
    if (width_low >= 0.0f) g_nib_width_low = width_low;

    float width_high = g_nib_width_high;
    cj_hw_read_float_key(buf, "\"hwStrokeNibWidthHigh\"", &width_high);
    if (width_high >= 0.0f) g_nib_width_high = width_high;
}

/* ctx 里 FUN_00f47530 自己维护的状态字段（跟点结构/点结构的方向字节无关，
 * 这个函数自己算自己的，见文件头注）：
 *   ctx+0x48/ctx+0x4c：上一个点的 x/y（float）
 *   ctx+0x5a：是不是这一笔的第一个点（非 0 = 已经有上一个点了）
 * offset 来自反编译交叉核对过的 FUN_00f47530 反汇编，不是猜的——跟"笔型
 * 标签"那次不一样，这几个字段是 FUN_00f47530 自己的代码直接用到、自己
 * 读写的，不管上游从哪条路径调进来都可靠（见上面的勘误说明）。 */
#define HW_CTX_LAST_X_OFF  0x48
#define HW_CTX_LAST_Y_OFF  0x4c
#define HW_CTX_HAS_PREV_OFF 0x5a

static float cj_hw_nib_ratio(float x, float y, void *ctx, float w) {
    if (g_nib_min_ratio >= 1.0f) return 1.0f; /* 关闭，省后面的计算 */
    if (*((uint8_t *)ctx + HW_CTX_HAS_PREV_OFF) == 0) return 1.0f; /* 笔画起点，没有上一个点算不出方向 */

    /* 按基础宽度算这次实际用的"最小比例"——w 越粗，越接近 g_nib_min_ratio
     * （满强度）；w 越细，越接近 1.0（趋近关闭）。 */
    float span = g_nib_width_high - g_nib_width_low;
    float t = (span > 0.0f) ? (w - g_nib_width_low) / span : 1.0f;
    if (t < 0.0f) t = 0.0f;
    if (t > 1.0f) t = 1.0f;
    float effective_min_ratio = 1.0f - t * (1.0f - g_nib_min_ratio);
    if (effective_min_ratio >= 1.0f) return 1.0f; /* 这支笔太细，效果已经趋近 0，省下面的三角函数 */

    float last_x = *(float *)((uint8_t *)ctx + HW_CTX_LAST_X_OFF);
    float last_y = *(float *)((uint8_t *)ctx + HW_CTX_LAST_Y_OFF);
    float dx = x - last_x;
    float dy = y - last_y;
    /* __builtin_sqrtf 编译成 ARM64 原生 FSQRT 指令，不经过 libm 符号——
     * sqrtf() 本身需要 GLIBC_2.43，设备 libm 没这么新，dlopen 直接解析失败
     * （真机实测撞到过，atan2f 也是同一个坑，见下面注释）。 */
    float len = __builtin_sqrtf(dx * dx + dy * dy);
    if (len == 0.0f) return 1.0f; /* 同一个点，方向未定义 */

    /* 不用 atan2f 求出真实角度再减——同样是 GLIBC_2.43 符号版本问题。用
     * sin(a-b)=sin(a)cos(b)-cos(a)sin(b) 展开绕开，sin(方向角)=dy/len、
     * cos(方向角)=dx/len，剩下只用 sinf/cosf/fabsf（GLIBC_2.17，跟其它
     * 符号一个级别，真机验证过能加载）。 */
    float sin_dir = dy / len;
    float cos_dir = dx / len;
    float nib_angle_rad = g_nib_angle_deg * (float)M_PI / 180.0f;
    float sin_diff = sin_dir * cosf(nib_angle_rad) - cos_dir * sinf(nib_angle_rad);
    float s = fabsf(sin_diff);
    return effective_min_ratio + (1.0f - effective_min_ratio) * s;
}

static void cj_hw_quad_handler(float x, float y, void *ctx) {
    cj_hw_refresh_config();

    float *width_ptr = (float *)((uint8_t *)ctx + 4);
    float w = *width_ptr;
    float nib_ratio = cj_hw_nib_ratio(x, y, ctx, w);

    fprintf(stderr, "[hw-stroke] x=%.1f y=%.1f w=%.4f factor=%.3f nib_ratio=%.3f\n",
            (double)x, (double)y, (double)w, (double)g_width_factor, (double)nib_ratio);

    if (g_width_factor != 1.0f || nib_ratio != 1.0f) {
        *width_ptr = w * g_width_factor * nib_ratio;
    }

    if (g_orig_hw_quad_call_through) {
        g_orig_hw_quad_call_through(x, y, ctx);
    }
}

static void cj_install_hw_quad_hook(uintptr_t target) {
    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_hw_quad_handler, &stub)) {
        fprintf(stderr, "[hw-stroke] 变宽几何 hook 安装失败（safe mode）\n");
        return;
    }
    g_orig_hw_quad_call_through = (orig_hw_quad_fn_t)stub;
    fprintf(stderr, "[hw-stroke] 变宽几何 hook 安装完成 @ %p（factor=%.3f）\n",
            (void *)target, (double)g_width_factor);
}

/* ---- 探查 FUN_00f3f9d0：纯诊断，零行为改动，不改任何值 ----
 *
 * 用户提出"CJK 顿挫不是单纯粗细问题"、查了社区（西式书法笔尖角度模型/中文
 * 毛笔提按时序模型）之后，下一步要确认笔尖角度模型能不能接上——公式是
 * `宽度 = 基础宽度 × |sin(方向角 − 笔尖固定角度)|`，方向角字节就在点结构
 * `offset 0xC`，FUN_00f3f9d0 已经在某个笔型分支里把它解出来存进
 * `*(float*)(ctx+8)`/`*(float*)(ctx+0xc)`（sin/cos，见反编译），但这是**条件
 * 触发的**（某标志位打开才算），不确定真机日常写字用的笔型会不会走到这条
 * 分支。这个诊断 hook 只读 `bVar16`（`*(byte*)(*param_1+0x70)`，决定走哪个
 * 宽度分支的笔型标签）和 `param_3`，原样调用穿透桩，不碰任何值——先把
 * "真机写字到底走哪条分支"坐实，再决定要不要在 cj_hw_quad_handler 里去读
 * ctx+8/ctx+0xc。 */

typedef void (*orig_hw_dispatch_fn_t)(void *, void *, int);
static orig_hw_dispatch_fn_t g_orig_hw_dispatch_call_through = NULL;

static void cj_hw_dispatch_handler(void *param_1, void *param_2, int param_3) {
    long lVar7 = *(long *)param_1;
    uint8_t bVar16 = *(uint8_t *)(lVar7 + 0x70);
    fprintf(stderr, "[hw-stroke-dispatch] param_3=%d bVar16=%u\n", param_3, (unsigned)bVar16);

    if (g_orig_hw_dispatch_call_through) {
        g_orig_hw_dispatch_call_through(param_1, param_2, param_3);
    }
}

static void cj_install_hw_dispatch_hook(uintptr_t target) {
    void *stub = NULL;
    if (!patch_target((void *)target, (void *)cj_hw_dispatch_handler, &stub)) {
        fprintf(stderr, "[hw-stroke] 分派诊断 hook 安装失败（safe mode，不影响变宽几何 hook）\n");
        return;
    }
    g_orig_hw_dispatch_call_through = (orig_hw_dispatch_fn_t)stub;
    fprintf(stderr, "[hw-stroke] 分派诊断 hook 安装完成 @ %p\n", (void *)target);
}

/* ---- xovi 扩展入口 ---- */

/* 固件兼容性判据是"变宽几何"这个主 hook 自己的目标特征码——这个是已经真机
 * 验证过、真正改变行为的 hook，决定整个扩展加不加载。诊断 hook（探查
 * FUN_00f3f9d0）是独立的、尽力而为的——找不到目标只跳过它自己，不拖累主
 * hook（跟 chinese-ime/langhook 8 个 hook 各自独立 install 同一个原则）。 */
char _xovi_shouldLoad(void) {
    uintptr_t base = 0, addr = 0;
    size_t size = 0;
    if (!cj_find_exec_module(TARGET_MODULE_SUFFIX, NULL, &base, &size)) {
        fprintf(stderr, "[hw-stroke] _xovi_shouldLoad: 找不到 xochitl 映射 → 拒绝加载(裸启原生)\n");
        return 0;
    }
    if (!cj_find_unique_pattern((const uint8_t *)base, size, PROLOGUE_HW_QUAD,
                                 sizeof(PROLOGUE_HW_QUAD), &addr)) {
        fprintf(stderr, "[hw-stroke] _xovi_shouldLoad: 变宽几何函数特征码未唯一命中"
                "(未知固件) → 拒绝加载(裸启原生)\n");
        return 0;
    }
    fprintf(stderr, "[hw-stroke] _xovi_shouldLoad: 固件兼容(FUN_00f47530@0x%lx) → 加载\n",
            (unsigned long)addr);
    return 1;
}

void _xovi_construct(void) {
    uintptr_t base = 0, addr = 0;
    size_t size = 0;
    if (!cj_find_exec_module(TARGET_MODULE_SUFFIX, NULL, &base, &size)) return;
    if (!cj_find_unique_pattern((const uint8_t *)base, size, PROLOGUE_HW_QUAD,
                                 sizeof(PROLOGUE_HW_QUAD), &addr)) {
        return; /* _xovi_shouldLoad 已经打过日志，这里不重复 */
    }
    cj_hw_refresh_config();
    cj_install_hw_quad_hook(addr);

    /* 诊断 hook 独立尝试安装，找不到目标只跳过（不影响上面主 hook 已经装好）。 */
    uintptr_t dispatch_addr = 0;
    if (cj_find_unique_pattern((const uint8_t *)base, size, PROLOGUE_HW_DISPATCH,
                                sizeof(PROLOGUE_HW_DISPATCH), &dispatch_addr)) {
        cj_install_hw_dispatch_hook(dispatch_addr);
    } else {
        fprintf(stderr, "[hw-stroke] 分派诊断函数特征码未唯一命中，跳过（不影响变宽几何 hook）\n");
    }
}
