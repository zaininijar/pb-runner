#include <dlfcn.h>
#include <stdio.h>
#include <stdint.h>
typedef struct {uint32_t sType; const void*pNext;uint32_t flags;const void*pApplicationInfo;uint32_t layerCount;const char*const*layers;uint32_t extensionCount;const char*const*extensions;} Info;
int main(int c,char**v){
 void*h=dlopen(v[1],RTLD_NOW|RTLD_LOCAL); if(!h){puts(dlerror());return 1;} puts("Vulkan loader: loaded");
 int(*create)(const Info*,void*,void**)=dlsym(h,"vkCreateInstance");
 int(*enumerate)(void*,uint32_t*,void*)=dlsym(h,"vkEnumeratePhysicalDevices");
 void(*destroy)(void*,void*)=dlsym(h,"vkDestroyInstance");
 const char*ext[]={"VK_KHR_portability_enumeration","VK_KHR_get_physical_device_properties2"}; Info i={1,0,1,0,0,0,2,ext};void*instance=0; int r=create(&i,0,&instance);printf("vkCreateInstance: %d\n",r);
 uint32_t n=0;if(!r){r=enumerate(instance,&n,0);printf("Physical devices: %u (result %d)\n",n,r);destroy(instance,0);}
 void*o=dlopen(v[2],RTLD_NOW|RTLD_LOCAL);if(!o){puts(dlerror());return 2;}puts("ODBC manager: loaded");
 short(*alloc)(short,void*,void**)=dlsym(o,"SQLAllocHandle");short(*release)(short,void*)=dlsym(o,"SQLFreeHandle");void*env=0;short s=alloc(1,0,&env);printf("SQLAllocHandle: %d\n",s);if(env)release(1,env);return (r||!n||s<0)?3:0;
}
