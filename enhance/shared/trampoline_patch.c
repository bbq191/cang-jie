#include "trampoline_patch.h"
#include "trampoline_aarch64.h"

#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <sys/mman.h>
#include <unistd.h>
#include <stdint.h>

static void *make_call_through_stub(const uint8_t *original_bytes, size_t patch_len, void *jump_back_target) {
    size_t stub_len = patch_len + CJ_FAR_JUMP_LEN;
    void *stub = mmap(NULL, stub_len, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (stub == MAP_FAILED) return NULL;

    memcpy(stub, original_bytes, patch_len);

    uint32_t jump_instrs[5];
    cj_build_far_jump(jump_instrs, jump_back_target);
    memcpy((uint8_t *)stub + patch_len, jump_instrs, CJ_FAR_JUMP_LEN);

    if (mprotect(stub, stub_len, PROT_READ | PROT_EXEC) != 0) {
        munmap(stub, stub_len);
        return NULL;
    }
    __builtin___clear_cache((char *)stub, (char *)stub + stub_len);
    return stub;
}

/* /proc/self/maps 里 addr 所在映射的权限（PROT_* 组合）；查不到返回 -1。 */
static int page_prot_of(uintptr_t addr) {
    FILE *f = fopen("/proc/self/maps", "r");
    if (!f) return -1;
    char line[512];
    int prot = -1;
    while (fgets(line, sizeof line, f)) {
        unsigned long lo, hi;
        char perms[5];
        if (sscanf(line, "%lx-%lx %4s", &lo, &hi, perms) != 3) continue;
        if (addr < lo || addr >= hi) continue;
        prot = (perms[0] == 'r' ? PROT_READ : 0) | (perms[1] == 'w' ? PROT_WRITE : 0) | (perms[2] == 'x' ? PROT_EXEC : 0);
        break;
    }
    fclose(f);
    return prot;
}

/* 只在原本是 r-x（可执行、不可写）时把页恢复回去；查不到或原本就可写则不动。 */
static void restore_prot(uintptr_t page_base, size_t region_len, int orig_prot, const char *tag) {
    if (orig_prot < 0 || (orig_prot & PROT_WRITE) || !(orig_prot & PROT_EXEC)) return;
    if (mprotect((void *)page_base, region_len, orig_prot) != 0) {
        fprintf(stderr, "[%s] 恢复代码页原权限失败，保持 rwx：%s\n", tag, strerror(errno));
    }
}

int cj_patch_target(void *target_addr, void *handler, size_t patch_len, const char *tag, void **out_stub) {
    long pagesize = sysconf(_SC_PAGESIZE);
    if (pagesize <= 0) pagesize = 4096;

    uintptr_t page_base = (uintptr_t)target_addr & ~((uintptr_t)pagesize - 1);
    size_t region_len = (size_t)pagesize;
    if ((((uintptr_t)target_addr - page_base) + patch_len) > region_len) {
        region_len += (size_t)pagesize;
    }

    /* 改之前记下原权限，改完恢复（2026-10-09）：代码页原本是 r-x，以前改完一直留着 rwx。两页权限不一致或查不到时
     * 不恢复（保持旧行为）；原本就可写（例如别的扩展先改过、留了 rwx）也不动——只把我们自己打开的写权限关回去。 */
    int orig_prot = page_prot_of(page_base);
    if (region_len > (size_t)pagesize && page_prot_of(page_base + (uintptr_t)pagesize) != orig_prot) orig_prot = -1;

    if (mprotect((void *)page_base, region_len, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
        fprintf(stderr, "[%s] mprotect 失败，放弃 hook（safe mode）：%s\n", tag, strerror(errno));
        return 0;
    }

    /* 被覆盖的指令要原样搬进调用桩：PC 相对的（adrp/b/bl/cbz/ldr literal…）搬过去地址全错，调用原函数时
     * 会跳飞或读错数据（2026-10-10，审计 EN-2）。放在 mprotect 之后检查：此时页一定可读（目标地址非法时上面已失败返回）。 */
    for (size_t off = 0; off + 4 <= patch_len; off += 4) {
        uint32_t insn;
        memcpy(&insn, (const uint8_t *)target_addr + off, sizeof insn);
        if (cj_insn_pc_relative(insn)) {
            fprintf(stderr, "[%s] 被覆盖的第 %zu 条指令 0x%08x 是 PC 相对寻址，搬进调用桩会算错地址，放弃 hook（safe mode）\n",
                    tag, off / 4 + 1, insn);
            restore_prot(page_base, region_len, orig_prot, tag);
            return 0;
        }
    }

    void *jump_back_target = (uint8_t *)target_addr + patch_len;
    void *stub = make_call_through_stub((const uint8_t *)target_addr, patch_len, jump_back_target);
    if (!stub) {
        fprintf(stderr, "[%s] 调用桩分配失败，放弃 hook（safe mode）\n", tag);
        restore_prot(page_base, region_len, orig_prot, tag);  /* 没改字节，也别把 rwx 留下 */
        return 0;
    }
    *out_stub = stub;

    uint32_t jump_to_handler[5];
    cj_build_far_jump(jump_to_handler, handler);
    memcpy(target_addr, jump_to_handler, CJ_FAR_JUMP_LEN);
    __builtin___clear_cache((char *)target_addr, (char *)target_addr + CJ_FAR_JUMP_LEN);

    restore_prot(page_base, region_len, orig_prot, tag);
    return 1;
}
