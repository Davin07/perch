$src = @'
using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;

[ComImport, Guid("62BE5D10-60EB-11d0-BD3B-00A0C911CE86"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface ICreateDevEnum {
    int CreateClassEnumerator([In] ref Guid pType, out IEnumMoniker ppEnumMoniker, int dwFlags);
}

[ComImport, Guid("55272A00-42CB-11CE-8135-00AA004BB851"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IPropertyBag {
    int Read(string pszPropName, out object pVar, IntPtr pErrorLog);
    int Write(string pszPropName, ref object pVar);
}

[ComImport, Guid("56A86895-0AD4-11CE-B03A-0020AF0BA770"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IBaseFilter { }

[ComImport, Guid("C6E13360-30AC-11d0-A18C-00A0C9118956"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IAMVideoProcAmp {
    int GetRange(int Property, out int pMin, out int pMax, out int pSteppingDelta, out int pDefault, out int pCapsFlags);
    int Set(int Property, int lValue, int Flags);
    int Get(int Property, out int lValue, out int Flags);
}

[ComImport, Guid("C6E13370-30AC-11d0-A18C-00A0C9118956"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
public interface IAMCameraControl {
    int GetRange(int Property, out int pMin, out int pMax, out int pSteppingDelta, out int pDefault, out int pCapsFlags);
    int Set(int Property, int lValue, int Flags);
    int Get(int Property, out int lValue, out int Flags);
}

public static class Uvc {
    static readonly string[] Vp = { "Brightness","Contrast","Hue","Saturation","Sharpness","Gamma","ColorEnable","WhiteBalance","BacklightComp","Gain" };
    static readonly string[] Cc = { "Pan","Tilt","Roll","Zoom","Exposure","Iris","Focus" };

    static void Dump(string label, object rangeObj) {
        int min, max, step, def, caps;
        var vp = rangeObj as IAMVideoProcAmp;
        if (vp != null) {
            for (int p = 0; p <= 9; p++) {
                try {
                    vp.GetRange(p, out min, out max, out step, out def, out caps);
                    int cur, flags; vp.Get(p, out cur, out flags);
                    Console.WriteLine("  {0,-10} cur={1,-6} min={2,-6} max={3,-6} step={4,-4} def={5,-6} auto={6} manual={7}",
                        Vp[p], cur, min, max, step, def, (caps & 1) != 0, (caps & 2) != 0);
                } catch { Console.WriteLine("  {0,-10} unsupported", Vp[p]); }
            }
        }
        var cc = rangeObj as IAMCameraControl;
        if (cc != null) {
            for (int p = 0; p <= 6; p++) {
                try {
                    cc.GetRange(p, out min, out max, out step, out def, out caps);
                    int cur, flags; cc.Get(p, out cur, out flags);
                    Console.WriteLine("  {0,-10} cur={1,-6} min={2,-6} max={3,-6} step={4,-4} def={5,-6} auto={6} manual={7}",
                        Cc[p], cur, min, max, step, def, (caps & 1) != 0, (caps & 2) != 0);
                } catch { Console.WriteLine("  {0,-10} unsupported", Cc[p]); }
            }
        }
    }

    public static void Run() {
        Guid cat = new Guid("860BB310-5D01-11d0-BD3B-00A0C911CE86");
        var devEnum = (ICreateDevEnum)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("62BE5D10-60EB-11d0-BD3B-00A0C911CE86")));
        IEnumMoniker e;
        devEnum.CreateClassEnumerator(ref cat, out e, 0);
        if (e == null) { Console.WriteLine("no video input devices"); return; }
        IMoniker[] m = new IMoniker[1];
        int n = 0;
        while (e.Next(1, m, IntPtr.Zero) == 0) {
            n++;
            string name = "?";
            try {
                object bagObj;
                m[0].BindToStorage(null, null, typeof(IPropertyBag).GUID, out bagObj);
                var bag = (IPropertyBag)bagObj;
                object nameObj;
                bag.Read("FriendlyName", out nameObj, IntPtr.Zero);
                if (nameObj != null) name = nameObj as string;
                Console.WriteLine("== " + name);
                object filterObj;
                m[0].BindToObject(null, null, typeof(IBaseFilter).GUID, out filterObj);
                var vp = filterObj as IAMVideoProcAmp;
                var cc = filterObj as IAMCameraControl;
                if (vp == null && cc == null) { Console.WriteLine("  no UVC control interfaces"); }
                if (vp != null) Dump("proc", vp);
                if (cc != null) Dump("cam", cc);
            } catch (Exception ex) {
                Console.WriteLine("== device " + n + " (" + name + "): " + ex.Message);
            }
        }
        if (n == 0) Console.WriteLine("enumerator returned no devices");
    }
}
'@

Add-Type -TypeDefinition $src -Language CSharp
[Uvc]::Run()
