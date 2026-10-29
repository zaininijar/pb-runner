using System;
using System.Runtime.InteropServices;
class D3D11Probe {
 [DllImport("d3d11.dll",CallingConvention=CallingConvention.StdCall)]static extern int D3D11CreateDevice(IntPtr adapter,uint type,IntPtr software,uint flags,IntPtr levels,uint numLevels,uint sdk,out IntPtr device,out uint level,out IntPtr context);
 static int Main(){IntPtr dev,ctx;uint level;int hr=D3D11CreateDevice(IntPtr.Zero,1,IntPtr.Zero,0,IntPtr.Zero,0,7,out dev,out level,out ctx);Console.WriteLine("D3D11 32-bit CreateDevice: 0x"+hr.ToString("X8")+"; feature level: 0x"+level.ToString("X"));if(ctx!=IntPtr.Zero)Marshal.Release(ctx);if(dev!=IntPtr.Zero)Marshal.Release(dev);return hr<0?1:0;}
}
