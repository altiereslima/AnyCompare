// External, application-neutral responsiveness meter (Windows).
// Starts a process, waits until it is idle, captures the window, then times
// mouse-wheel steps and typed characters: each step is SendMessage (synchronous)
// followed by RedrawWindow(RDW_UPDATENOW), so the time includes the repaint.
// Build: csc /nologo /out:perf_harness.exe /r:System.Drawing.dll perf_harness.cs
// Usage: perf_harness <out-prefix> <exe> [args...]
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

static class Harness
{
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hgt, uint flags);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(POINT p);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern bool RedrawWindow(IntPtr h, IntPtr rc, IntPtr rgn, uint flags);
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] static extern bool ScreenToClient(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] struct POINT { public int X, Y; }
    const uint WM_MOUSEMOVE = 0x200, WM_LBUTTONDOWN = 0x201, WM_LBUTTONUP = 0x202,
        WM_MOUSEWHEEL = 0x20A, WM_CHAR = 0x102, WM_KEYDOWN = 0x100, WM_KEYUP = 0x101;
    const uint RDW_INVALIDATE = 1, RDW_UPDATENOW = 0x100, RDW_ALLCHILDREN = 0x80;

    static IntPtr MainWindow(int pid)
    {
        IntPtr best = IntPtr.Zero; long area = 0;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            RECT r;
            if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r)) {
                long a = (long)(r.R - r.L) * (r.B - r.T);
                if (a > area) { area = a; best = h; }
            }
            return true;
        }, IntPtr.Zero);
        return best;
    }

    static IntPtr Lp(int x, int y) { return (IntPtr)((y << 16) | (x & 0xffff)); }

    static void WaitIdle(Process p, int quietMs)
    {
        TimeSpan last = TimeSpan.Zero; var quiet = Stopwatch.StartNew(); var total = Stopwatch.StartNew();
        // Some OpenGL drivers spin while idle: never wait longer than the limit.
        int limit = int.Parse(Environment.GetEnvironmentVariable("PERF_IDLE_LIMIT_MS") ?? "60000");
        while (total.ElapsedMilliseconds < limit) {
            Thread.Sleep(50);
            p.Refresh();
            var cpu = p.TotalProcessorTime;
            if (cpu - last > TimeSpan.FromMilliseconds(15)) { last = cpu; quiet.Restart(); }
            else if (quiet.ElapsedMilliseconds >= quietMs) return;
        }
    }

    static void Capture(IntPtr h, string path)
    {
        RECT r; GetWindowRect(h, out r);
        Console.Error.WriteLine("capture " + h + " " + r.L + "," + r.T + "," + r.R + "," + r.B);
        if (r.R - r.L <= 0 || r.B - r.T <= 0) return;
        using (var bmp = new Bitmap(r.R - r.L, r.B - r.T))
        using (var g = Graphics.FromImage(bmp)) {
            var hdc = g.GetHdc(); PrintWindow(h, hdc, 2); g.ReleaseHdc(hdc);
            bmp.Save(path, ImageFormat.Png);
        }
    }

    static string Stats(List<double> v)
    {
        var s = v.OrderBy(x => x).ToList();
        return string.Format(System.Globalization.CultureInfo.InvariantCulture,
            "{{\"count\":{0},\"mean_ms\":{1:F2},\"p95_ms\":{2:F2},\"max_ms\":{3:F2},\"total_ms\":{4:F1} }}",
            v.Count, v.Average(), s[s.Count * 95 / 100 >= s.Count ? s.Count - 1 : s.Count * 95 / 100], s.Last(), v.Sum());
    }

    static int Main(string[] args)
    {
        string prefix = args[0];
        var psi = new ProcessStartInfo(args[1], string.Join(" ", args.Skip(2).Select(a => "\"" + a + "\"")));
        psi.UseShellExecute = false;
        // Isolated profile folder: the measured app never touches the user's settings.
        var appdata = System.IO.Path.GetFullPath(prefix + "-appdata");
        System.IO.Directory.CreateDirectory(appdata);
        psi.EnvironmentVariables["APPDATA"] = appdata;
        psi.WorkingDirectory = System.IO.Path.GetDirectoryName(System.IO.Path.GetFullPath(args[1]));
        var clock = Stopwatch.StartNew();
        var p = Process.Start(psi);
        IntPtr win = IntPtr.Zero;
        while ((win = MainWindow(p.Id)) == IntPtr.Zero) Thread.Sleep(10);
        double windowMs = clock.Elapsed.TotalMilliseconds;
        WaitIdle(p, 1500);
        double idleMs = clock.Elapsed.TotalMilliseconds - 1500;
        win = MainWindow(p.Id);
        ShowWindow(win, 4 /*SW_SHOWNOACTIVATE: restore a minimized window*/);
        SetWindowPos(win, IntPtr.Zero, 100, 60, 1600, 940, 0x0010 /*NOACTIVATE*/ | 0x0004 /*NOZORDER*/);
        WaitIdle(p, 800);
        Capture(win, prefix + "-open.png");
        RECT r; GetWindowRect(win, out r);
        // Left pane: 30% of the width, middle of the height.
        var pt = new POINT { X = r.L + (r.R - r.L) * 30 / 100, Y = r.T + (r.B - r.T) / 2 };
        IntPtr target = WindowFromPoint(pt);
        var cls = new StringBuilder(256); GetClassName(target, cls, 256);
        var client = pt; ScreenToClient(target, ref client);
        SendMessage(target, WM_MOUSEMOVE, IntPtr.Zero, Lp(client.X, client.Y));
        var down = new List<double>(); var up = new List<double>();
        for (int pass = 0; pass < 2; pass++) {
            var list = pass == 0 ? down : up;
            int delta = pass == 0 ? -120 : 120;
            for (int i = 0; i < 60; i++) {
                var t = Stopwatch.StartNew();
                SendMessage(target, WM_MOUSEWHEEL, (IntPtr)(delta << 16), Lp(pt.X, pt.Y));
                RedrawWindow(win, IntPtr.Zero, IntPtr.Zero, RDW_UPDATENOW | RDW_ALLCHILDREN);
                list.Add(t.Elapsed.TotalMilliseconds);
            }
            if (pass == 0) Capture(win, prefix + "-scrolled.png");
        }
        // Caret in the left pane, a few characters into the visible top line.
        var caret = new POINT { X = r.L + (r.R - r.L) * 12 / 100, Y = r.T + (r.B - r.T) * 30 / 100 };
        IntPtr editor = WindowFromPoint(caret);
        var cc = caret; ScreenToClient(editor, ref cc);
        SendMessage(editor, WM_MOUSEMOVE, IntPtr.Zero, Lp(cc.X, cc.Y));
        SendMessage(editor, WM_LBUTTONDOWN, (IntPtr)1, Lp(cc.X, cc.Y));
        SendMessage(editor, WM_LBUTTONUP, IntPtr.Zero, Lp(cc.X, cc.Y));
        RedrawWindow(win, IntPtr.Zero, IntPtr.Zero, RDW_UPDATENOW | RDW_ALLCHILDREN);
        WaitIdle(p, 500);
        var typing = new List<double>();
        for (int i = 0; i < 20; i++) {
            var t = Stopwatch.StartNew();
            SendMessage(editor, WM_KEYDOWN, (IntPtr)0x58, (IntPtr)0x002D0001);
            SendMessage(editor, WM_CHAR, (IntPtr)'x', (IntPtr)0x002D0001);
            SendMessage(editor, WM_KEYUP, (IntPtr)0x58, unchecked((IntPtr)(int)0xC02D0001));
            RedrawWindow(win, IntPtr.Zero, IntPtr.Zero, RDW_UPDATENOW | RDW_ALLCHILDREN);
            typing.Add(t.Elapsed.TotalMilliseconds);
        }
        var settle = Stopwatch.StartNew();
        WaitIdle(p, 800);
        double settleMs = settle.Elapsed.TotalMilliseconds - 800;
        Capture(win, prefix + "-typed.png");
        Console.WriteLine(string.Format(System.Globalization.CultureInfo.InvariantCulture,
            "{{\"exe\":\"{0}\",\"window_ms\":{1:F0},\"idle_after_start_ms\":{2:F0},\"wheel_target\":\"{3}\",\"scroll_down\":{4},\"scroll_up\":{5},\"typing\":{6},\"typing_settle_ms\":{7:F0} }}",
            System.IO.Path.GetFileName(args[1]), windowMs, idleMs, cls, Stats(down), Stats(up), Stats(typing), settleMs));
        p.Kill();
        p.WaitForExit();
        return 0;
    }
}
