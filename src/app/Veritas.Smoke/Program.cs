using Veritas.Core.Engine;
using Veritas.Core.Models;

using var engine = new NativeEngine();
Console.WriteLine("HEALTH: " + engine.Health());

try
{
    var req = new AnalyzeRequest(
        Text: "VIRAL! Vaksin COVID-19 mengandung microchip 5G dan pemerintah menyembunyikan faktanya. Sebarkan sebelum dihapus!",
        LangHint: "id",
        Options: new AnalyzeOptions(OnlineEvidence: true));
    var resp = engine.Analyze(req, ev => Console.WriteLine($"PROGRESS {ev.Stage} {ev.Percent:P0} {ev.Message}"));
    Console.WriteLine("VERDICT: " + resp.Verdict);
    Console.WriteLine("CONFIDENCE: " + resp.Confidence);
    Console.WriteLine("TRUTH: " + resp.TruthPercentage + "%");
    Console.WriteLine("SUMMARY: " + resp.Summary);
    Console.WriteLine("CLAIMS: " + resp.Claims.Length);
    Console.WriteLine("EVIDENCE: " + resp.Evidence.Length);
    foreach (var item in resp.Evidence)
    {
        Console.WriteLine($"  [{item.Id}] {item.Kind} {item.Source} {item.Url}");
    }
    foreach (var warning in resp.Warnings)
    {
        Console.WriteLine("WARNING: " + warning);
    }
    Console.WriteLine("OK");
}
catch (Exception ex)
{
    Console.WriteLine("FAILED: " + ex.Message);
    Environment.ExitCode = 1;
}
