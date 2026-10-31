using Veritas.Core.Engine;
using Veritas.Core.Models;

var engine = new NativeEngine();
Console.WriteLine("HEALTH: " + engine.Health());

var req = new AnalyzeRequest(Text: "Ini adalah berita hoax palsu yang harus diviralkan sekarang!", LangHint: "id");
var resp = engine.Analyze(req);
Console.WriteLine("VERDICT: " + resp.Verdict);
Console.WriteLine("CONFIDENCE: " + resp.Confidence);
Console.WriteLine("SUMMARY: " + resp.Summary);
Console.WriteLine("CLAIMS: " + resp.Claims.Length);
engine.Dispose();
Console.WriteLine("OK");
