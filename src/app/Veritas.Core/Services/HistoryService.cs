using System.Text.Json;
using Microsoft.Data.Sqlite;

namespace Veritas.Core.Services;

public sealed record HistoryEntry(
    long Id,
    string Input,
    string Verdict,
    double Confidence,
    string Summary,
    string CreatedAt
);

public sealed class HistoryService : IDisposable
{
    private readonly SqliteConnection _conn;

    public HistoryService(string dbPath)
    {
        var dir = Path.GetDirectoryName(dbPath);
        if (!string.IsNullOrEmpty(dir))
        {
            Directory.CreateDirectory(dir);
        }
        _conn = new SqliteConnection($"Data Source={dbPath}");
        _conn.Open();
        using var cmd = _conn.CreateCommand();
        cmd.CommandText = """
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                input TEXT NOT NULL,
                verdict TEXT NOT NULL,
                confidence REAL NOT NULL,
                summary TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            """;
        cmd.ExecuteNonQuery();
    }

    public void Add(string input, string verdict, double confidence, string summary)
    {
        using var cmd = _conn.CreateCommand();
        cmd.CommandText = "INSERT INTO history (input, verdict, confidence, summary, created_at) VALUES ($i,$v,$c,$s,$t);";
        cmd.Parameters.AddWithValue("$i", input);
        cmd.Parameters.AddWithValue("$v", verdict);
        cmd.Parameters.AddWithValue("$c", confidence);
        cmd.Parameters.AddWithValue("$s", summary);
        cmd.Parameters.AddWithValue("$t", DateTime.UtcNow.ToString("O"));
        cmd.ExecuteNonQuery();
    }

    public List<HistoryEntry> List(int limit = 200)
    {
        var list = new List<HistoryEntry>();
        using var cmd = _conn.CreateCommand();
        cmd.CommandText = "SELECT id, input, verdict, confidence, summary, created_at FROM history ORDER BY id DESC LIMIT $l;";
        cmd.Parameters.AddWithValue("$l", limit);
        using var r = cmd.ExecuteReader();
        while (r.Read())
        {
            list.Add(new HistoryEntry(
                r.GetInt64(0),
                r.GetString(1),
                r.GetString(2),
                r.GetDouble(3),
                r.GetString(4),
                r.GetString(5)));
        }
        return list;
    }

    public void Clear()
    {
        using var cmd = _conn.CreateCommand();
        cmd.CommandText = "DELETE FROM history;";
        cmd.ExecuteNonQuery();
    }

    public void Dispose() => _conn.Dispose();
}
