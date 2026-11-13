// Samples the instruction pointer of a process's busiest thread and prints the
// most frequent functions (dbghelp + the executable's PDB). Diagnostic only.
// Build: csc /nologo /out:stack_sampler.exe stack_sampler.cs
// Usage: stack_sampler <pid> [samples]
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

static class Sampler
{
    [DllImport("kernel32.dll")] static extern IntPtr OpenThread(uint access, bool inherit, uint id);
    [DllImport("kernel32.dll")] static extern uint SuspendThread(IntPtr h);
    [DllImport("kernel32.dll")] static extern uint ResumeThread(IntPtr h);
    [DllImport("kernel32.dll")] static extern bool GetThreadContext(IntPtr h, IntPtr ctx);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr p, IntPtr addr, byte[] buf, IntPtr size, out IntPtr read);
    [DllImport("dbghelp.dll", CharSet = CharSet.Unicode)] static extern bool SymInitializeW(IntPtr p, string path, bool invade);
    [DllImport("dbghelp.dll")] static extern uint SymSetOptions(uint o);
    [DllImport("dbghelp.dll", CharSet = CharSet.Unicode)] static extern bool SymFromAddrW(IntPtr p, ulong addr, out ulong disp, IntPtr info);

    static string Name(IntPtr proc, ulong addr)
    {
        IntPtr info = Marshal.AllocHGlobal(8 + 1024 * 2 + 88);
        try {
            for (int i = 0; i < 88 + 2048; i++) Marshal.WriteByte(info, i, 0);
            Marshal.WriteInt32(info, 0, 88);          // SizeOfStruct (SYMBOL_INFOW)
            Marshal.WriteInt32(info, 80, 1000);       // MaxNameLen
            ulong disp;
            if (SymFromAddrW(proc, addr, out disp, info)) {
                int len = Marshal.ReadInt32(info, 76);
                return Marshal.PtrToStringUni(IntPtr.Add(info, 84), len);
            }
        } finally { Marshal.FreeHGlobal(info); }
        return "0x" + addr.ToString("X");
    }

    static int Main(string[] args)
    {
        int pid = int.Parse(args[0]);
        int samples = args.Length > 1 ? int.Parse(args[1]) : 300;
        var p = Process.GetProcessById(pid);
        var before = p.Threads.Cast<ProcessThread>().ToDictionary(t => t.Id, t => t.TotalProcessorTime);
        Thread.Sleep(1000);
        p.Refresh();
        var busiest = p.Threads.Cast<ProcessThread>()
            .OrderByDescending(t => t.TotalProcessorTime - (before.ContainsKey(t.Id) ? before[t.Id] : TimeSpan.Zero))
            .First();
        IntPtr proc = OpenProcess(0x1F0FFF, false, pid);
        SymSetOptions(0x2 | 0x4 | 0x10); // UNDNAME | DEFERRED_LOADS | LOAD_LINES
        SymInitializeW(proc, null, true);
        IntPtr th = OpenThread(0x1FFFFF, false, (uint)busiest.Id);
        IntPtr ctx = Marshal.AllocHGlobal(1232 + 16);
        IntPtr aligned = new IntPtr((ctx.ToInt64() + 15) & ~15L);
        var counts = new Dictionary<string, int>();
        var callers = new Dictionary<string, int>();
        for (int i = 0; i < samples; i++) {
            SuspendThread(th);
            Marshal.WriteInt32(aligned, 0x30, 0x10001B); // ContextFlags = CONTROL|INTEGER|SEGMENTS (AMD64)
            if (GetThreadContext(th, aligned)) {
                ulong rip = (ulong)Marshal.ReadInt64(aligned, 0xF8);
                ulong rsp = (ulong)Marshal.ReadInt64(aligned, 0x98);
                string n = Name(proc, rip);
                counts[n] = counts.ContainsKey(n) ? counts[n] + 1 : 1;
                // Crude caller hint: symbolic return addresses found on the stack top.
                var buf = new byte[8 * 64]; IntPtr read;
                if (ReadProcessMemory(proc, (IntPtr)(long)rsp, buf, (IntPtr)buf.Length, out read)) {
                    var seen = new HashSet<string>();
                    for (int k = 0; k < 64; k++) {
                        ulong v = BitConverter.ToUInt64(buf, k * 8);
                        if (v < 0x10000) continue;
                        string c = Name(proc, v);
                        if (c.StartsWith("0x") || !seen.Add(c)) continue;
                        callers[c] = callers.ContainsKey(c) ? callers[c] + 1 : 1;
                    }
                }
            }
            ResumeThread(th);
            Thread.Sleep(5);
        }
        Console.WriteLine("thread " + busiest.Id);
        foreach (var kv in counts.OrderByDescending(k => k.Value).Take(15)) Console.WriteLine("ip " + kv.Value + " " + kv.Key);
        foreach (var kv in callers.OrderByDescending(k => k.Value).Take(40)) Console.WriteLine("stack " + kv.Value + " " + kv.Key);
        return 0;
    }
}
