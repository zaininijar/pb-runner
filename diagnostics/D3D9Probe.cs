using System;
using System.Runtime.InteropServices;
using System.Windows.Forms;
class D3D9Probe {
 [DllImport("d3d9.dll", CallingConvention=CallingConvention.StdCall)] static extern IntPtr Direct3DCreate9(uint sdk);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate uint AdapterCount(IntPtr self);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int CreateDevice(IntPtr self,uint adapter,uint type,IntPtr window,uint flags,ref Parameters pp,out IntPtr device);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int Clear(IntPtr self,uint count,IntPtr rects,uint flags,uint color,float z,uint stencil);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int Present(IntPtr self,IntPtr source,IntPtr dest,IntPtr window,IntPtr dirty);
 [StructLayout(LayoutKind.Sequential)] struct Parameters {public uint width,height,format,count,multisample,quality,swap;public IntPtr window;public int windowed,depth;public uint depthFormat,flags,refresh,interval;}
 static T Method<T>(IntPtr obj,int index) {return (T)(object)Marshal.GetDelegateForFunctionPointer(Marshal.ReadIntPtr(Marshal.ReadIntPtr(obj),index*IntPtr.Size),typeof(T));}
 [STAThread] static int Main() {
  Console.WriteLine("D3D9 probe "+(IntPtr.Size*8)+"-bit");
  using(var form=new Form()) {
   form.Text="PB Runner — tes D3D9";form.ClientSize=new System.Drawing.Size(640,480);form.Show();Application.DoEvents();
   IntPtr d3d=Direct3DCreate9(32);if(d3d==IntPtr.Zero){Console.WriteLine("Direct3DCreate9 failed");return 1;}
   Console.WriteLine("Adapters: "+Method<AdapterCount>(d3d,4)(d3d));
   var pp=new Parameters{width=640,height=480,count=1,swap=1,window=form.Handle,windowed=1,interval=0x80000000};IntPtr dev;
   int hr=Method<CreateDevice>(d3d,16)(d3d,0,1,form.Handle,0x20,ref pp,out dev);Console.WriteLine("CreateDevice: 0x"+hr.ToString("X8"));
   if(hr<0){Marshal.Release(d3d);return 2;}
   int clear=Method<Clear>(dev,43)(dev,0,IntPtr.Zero,1,0xff2878a0,1,0);
   int present=Method<Present>(dev,17)(dev,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero,IntPtr.Zero);
   Console.WriteLine("Clear: 0x"+clear.ToString("X8")+"; Present: 0x"+present.ToString("X8"));
   Application.DoEvents();Marshal.Release(dev);Marshal.Release(d3d);return clear<0||present<0?3:0;
  }
 }
}
