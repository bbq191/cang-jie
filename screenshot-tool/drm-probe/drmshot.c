/* DRM 截屏可行性实验：读 xochitl 当前扫描的 framebuffer。
 * 纯 ioctl（不依赖 libdrm），静态编译。root 跑。
 * 输出 fb 元信息 + dump 原始像素到 /tmp/drmshot.raw + 尝试转 /tmp/drmshot.ppm。 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>
#include <stdint.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <drm/drm.h>
#include <drm/drm_mode.h>

int main(void) {
    int fd = open("/dev/dri/card0", O_RDWR | O_CLOEXEC);
    if (fd < 0) { perror("open card0"); return 1; }

    /* GETRESOURCES：两遍——先拿 counts，再为所有数组提供缓冲（DRM 要求全给，否则 EFAULT） */
    struct drm_mode_card_res res;
    memset(&res, 0, sizeof(res));
    if (ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res)) { perror("GETRESOURCES(count)"); return 1; }
    printf("counts: fbs=%u crtcs=%u conns=%u encs=%u  wxh=%u..%u x %u..%u\n",
           res.count_fbs, res.count_crtcs, res.count_connectors, res.count_encoders,
           res.min_width, res.max_width, res.min_height, res.max_height);
    uint32_t fbs[64], crtcs[64], conns[64], encs[64];
    if (res.count_fbs > 64) res.count_fbs = 64;
    if (res.count_crtcs > 64) res.count_crtcs = 64;
    if (res.count_connectors > 64) res.count_connectors = 64;
    if (res.count_encoders > 64) res.count_encoders = 64;
    res.fb_id_ptr = res.count_fbs ? (uint64_t)(uintptr_t)fbs : 0;
    res.crtc_id_ptr = res.count_crtcs ? (uint64_t)(uintptr_t)crtcs : 0;
    res.connector_id_ptr = res.count_connectors ? (uint64_t)(uintptr_t)conns : 0;
    res.encoder_id_ptr = res.count_encoders ? (uint64_t)(uintptr_t)encs : 0;
    if (ioctl(fd, DRM_IOCTL_MODE_GETRESOURCES, &res)) { perror("GETRESOURCES"); return 1; }
    printf("crtcs=%u connectors=%u\n", res.count_crtcs, res.count_connectors);

    uint32_t fb_id = 0, cw = 0, ch = 0;
    for (unsigned i = 0; i < res.count_crtcs; i++) {
        struct drm_mode_crtc crtc;
        memset(&crtc, 0, sizeof(crtc));
        crtc.crtc_id = crtcs[i];
        if (ioctl(fd, DRM_IOCTL_MODE_GETCRTC, &crtc)) continue;
        printf("crtc %u: fb_id=%u mode_valid=%u %ux%u\n",
               crtc.crtc_id, crtc.fb_id, crtc.mode_valid, crtc.mode.hdisplay, crtc.mode.vdisplay);
        if (crtc.fb_id) { fb_id = crtc.fb_id; cw = crtc.mode.hdisplay; ch = crtc.mode.vdisplay; break; }
    }
    if (!fb_id) { fprintf(stderr, "no active crtc fb\n"); return 2; }

    /* GETFB2：拿 fb 的宽高/格式/handle/pitch/modifier */
    struct drm_mode_fb_cmd2 fb;
    memset(&fb, 0, sizeof(fb));
    fb.fb_id = fb_id;
    if (ioctl(fd, DRM_IOCTL_MODE_GETFB2, &fb)) { perror("GETFB2"); return 3; }
    char fourcc[5] = {0};
    memcpy(fourcc, &fb.pixel_format, 4);
    printf("fb %u: %ux%u fmt='%s'(0x%08x)\n", fb_id, fb.width, fb.height, fourcc, fb.pixel_format);
    for (int i = 0; i < 4; i++)
        if (fb.handles[i] || fb.pitches[i])
            printf("  plane%d: handle=%u pitch=%u offset=%u modifier=0x%llx\n",
                   i, fb.handles[i], fb.pitches[i], fb.offsets[i],
                   (unsigned long long)fb.modifier[i]);

    if (!fb.handles[0]) { fprintf(stderr, "handle=0 (可能需要 DRM master / 权限不足)\n"); return 4; }

    /* handle → dmabuf fd → mmap 读 */
    struct drm_prime_handle prime;
    memset(&prime, 0, sizeof(prime));
    prime.handle = fb.handles[0];
    prime.flags = DRM_CLOEXEC;
    if (ioctl(fd, DRM_IOCTL_PRIME_HANDLE_TO_FD, &prime)) { perror("PRIME_HANDLE_TO_FD"); return 5; }

    size_t size = (size_t)fb.pitches[0] * fb.height;
    void *map = mmap(NULL, size, PROT_READ, MAP_SHARED, prime.fd, 0);
    if (map == MAP_FAILED) { perror("mmap"); return 6; }

    FILE *raw = fopen("/tmp/drmshot.raw", "wb");
    if (raw) { fwrite(map, 1, size, raw); fclose(raw); }
    printf("dumped %zu bytes -> /tmp/drmshot.raw\n", size);

    /* 粗略转 PPM：按 32bpp BGRX→RGB（线性假设；若 modifier!=0 是 tiled 会花，看元信息再说） */
    unsigned bpp = fb.width ? fb.pitches[0] / fb.width : 0;
    printf("approx bytes/pixel=%u  modifier[0]=0x%llx (0=线性)\n", bpp, (unsigned long long)fb.modifier[0]);
    if (bpp == 4) {
        FILE *ppm = fopen("/tmp/drmshot.ppm", "wb");
        if (ppm) {
            fprintf(ppm, "P6\n%u %u\n255\n", fb.width, fb.height);
            uint8_t *base = (uint8_t *)map;
            for (uint32_t y = 0; y < fb.height; y++) {
                uint8_t *row = base + (size_t)y * fb.pitches[0];
                for (uint32_t x = 0; x < fb.width; x++) {
                    uint8_t *px = row + (size_t)x * 4;
                    uint8_t rgb[3] = { px[2], px[1], px[0] };
                    fwrite(rgb, 1, 3, ppm);
                }
            }
            fclose(ppm);
            printf("wrote /tmp/drmshot.ppm (%ux%u)\n", fb.width, fb.height);
        }
    }
    (void)cw; (void)ch;
    return 0;
}
