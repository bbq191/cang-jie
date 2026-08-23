#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <xf86drm.h>
#include <xf86drmMode.h>
int main(void){
    int fd=open("/dev/dri/card0",O_RDWR|O_CLOEXEC);
    if(fd<0){perror("open");return 1;}
    drmSetClientCap(fd,DRM_CLIENT_CAP_UNIVERSAL_PLANES,1);
    drmModePlaneRes*pr=drmModeGetPlaneResources(fd);
    if(!pr){perror("GetPlaneResources");return 1;}
    printf("planes=%u\n",pr->count_planes);
    uint32_t fb_id=0;
    for(uint32_t i=0;i<pr->count_planes;i++){
        drmModePlane*pl=drmModeGetPlane(fd,pr->planes[i]); if(!pl)continue;
        if(pl->fb_id){printf("plane %u fb_id=%u crtc=%u\n",pl->plane_id,pl->fb_id,pl->crtc_id); if(!fb_id)fb_id=pl->fb_id;}
        drmModeFreePlane(pl);
    }
    if(!fb_id){fprintf(stderr,"no plane with fb\n");return 2;}
    drmModeFB2*fb=drmModeGetFB2(fd,fb_id);
    if(!fb){perror("GetFB2(非master被挡?)");return 3;}
    printf("FB2 OK: %ux%u fmt=0x%08x handle0=%u pitch0=%u mod=0x%llx\n",
        fb->width,fb->height,fb->pixel_format,fb->handles[0],fb->pitches[0],(unsigned long long)fb->modifier);
    if(!fb->handles[0]){fprintf(stderr,"handle0=0 非master拿不到buffer\n");return 4;}
    int dfd=-1;
    if(drmPrimeHandleToFD(fd,fb->handles[0],DRM_CLOEXEC,&dfd)){perror("PrimeHandleToFD");return 5;}
    size_t sz=(size_t)fb->pitches[0]*fb->height;
    void*m=mmap(NULL,sz,PROT_READ,MAP_SHARED,dfd,0);
    if(m==MAP_FAILED){perror("mmap");return 6;}
    FILE*r=fopen("/tmp/drmshot.raw","wb"); if(r){fwrite(m,1,sz,r);fclose(r);}
    printf("READ OK %zu bytes -> /tmp/drmshot.raw\n",sz);
    return 0;
}
