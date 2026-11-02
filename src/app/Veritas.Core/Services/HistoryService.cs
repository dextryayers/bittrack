using System.Globalization;
using System.Text.Json;
using Microsoft.Data.Sqlite;

namespace Veritas.Core.Services;

public sealed record HistoryEntry(
    long Id,
    string Input,
    string Verdict,
    double Confidence,
    string Summary,
    string CreatedAt,
    string ResultJson = ""
)
{
    public string CreatedAtDisplay =>
        DateTime.TryParse(CreatedAt, CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind, out var parsed)
            ? parsed.ToLocalTime().ToString("dd MMM yyyy HH:mm", CultureInfo.InvariantCulture)
            : CreatedAt;

    public string InputPreview =>
        Input.Length <= 180 ? Input : Input[..180] + "…";

    public string ConfidenceDisplay => $"{Confidence * 100:0}%";

    public string VerdictDisplay => Verdict switch
    {
        "hoax" => "Hoaks",
        "misleading" => "Menyesatkan",
        "clickbait" => "Clickbait",
        "buzzer" => "Buzzer",
        "valid" => "Valid",
        "unverifiable" => "Tidak terverifikasi",
        _ => Verdict,
    };

    public bool CanReopen => !string.IsNullOrWhiteSpace(ResultJson);
}

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
        Migrate();
    }

    /// <summary>
    /// Adds the full result JSON column to databases created by earlier builds.
    /// Without it, reopening a history item is impossible and the UI would lie
    /// by offering an action that cannot work.
    /// </summary>
    private void Migrate()
    {
        bool hasResultColumn = false;
        using (var check = _conn.CreateCommand())
        {
            check.CommandText = "PRAGMA table_info(history);";
            using var reader = check.ExecuteReader();
            while (reader.Read())
            {
                if (string.Equals(reader.GetString(1), "result_json", StringComparison.OrdinalIgnoreCase))
                {
                    hasResultColumn = true;
                }
            }
        }

        if (hasResultColumn)
        {
            return;
        }

        using var alter = _conn.CreateCommand();
        alter.CommandText = "ALTER TABLE history ADD COLUMN result_json TEXT NOT NULL DEFAULT '';";
        alter.ExecuteNonQuery();
    }

    public void Add(string input, string verdict, double confidence, string summary, string resultJson = "")
    {
        using var cmd = _conn.CreateCommand();
        cmd.CommandText =
            "INSERT INTO history (input, verdict, confidence, summary, created_at, result_json) " +
            "VALUES ($i,$v,$c,$s,$t,$r);";
        cmd.Parameters.AddWithValue("$i", input);
        cmd.Parameters.AddWithValue("$v", verdict);
        cmd.Parameters.AddWithValue("$c", confidence);
        cmd.Parameters.AddWithValue("$s", summary);
        cmd.Parameters.AddWithValue("$t", DateTime.UtcNow.ToString("O"));
        cmd.Parameters.AddWithValue("$r", resultJson);
        cmd.ExecuteNonQuery();
    }

    public List<HistoryEntry> List(int limit = 200) => Search(null, limit);

    /// <summary>
    /// Case-insensitive substring search over the stored input and verdict.
    /// Uses `instr` instead of LIKE so user input is never treated as a wildcard.
    /// </summary>
    public List<HistoryEntry> Search(string? query, int limit = 200)
    {
        var list = new List<HistoryEntry>();
        using var cmd = _conn.CreateCommand();
        if (string.IsNullOrWhiteSpace(query))
        {
            cmd.CommandText =
                "SELECT id, input, verdict, confidence, summary, created_at, result_json " +
                "FROM history ORDER BY id DESC LIMIT $l;";
        }
        else
        {
            cmd.CommandText =
                "SELECT id, input, verdict, confidence, summary, created_at, result_json " +
                "FROM history " +
                "WHERE instr(lower(input), lower($q)) > 0 OR instr(lower(verdict), lower($q)) > 0 " +
                "ORDER BY id DESC LIMIT $l;";
            cmd.Parameters.AddWithValue("$q", query.Trim());
        }
        cmd.Parameters.AddWithValue("$l", Math.Clamp(limit, 1, 1000));
        using var r = cmd.ExecuteReader();
        while (r.Read())
        {
            list.Add(new HistoryEntry(
                r.GetInt64(0),
                r.GetString(1),
                r.GetString(2),
                r.GetDouble(3),
                r.GetString(4),
                r.GetString(5),
                r.GetString(6)));
        }
        return list;
    }

    public HistoryEntry? Get(long id)
    {
        using var cmd = _conn.CreateCommand();
        cmd.CommandText =
            "SELECT id, input, verdict, confidence, summary, created_at, result_json " +
            "FROM history WHERE id = $id LIMIT 1;";
        cmd.Parameters.AddWithValue("$id", id);
        using var r = cmd.ExecuteReader();
        return r.Read()
            ? new HistoryEntry(r.GetInt64(0), r.GetString(1), r.GetString(2), r.GetDouble(3),
                r.GetString(4), r.GetString(5), r.GetString(6))
            : null;
    }

    public bool Delete(long id)
    {
        using var cmd = _conn.CreateCommand();
        cmd.CommandText = "DELETE FROM history WHERE id = $id;";
        cmd.Parameters.AddWithValue("$id", id);
        return cmd.ExecuteNonQuery() > 0;
    }

    public void Clear()
    {
        using var cmd = _conn.CreateCommand();
        cmd.CommandText = "DELETE FROM history;";
        cmd.ExecuteNonQuery();
    }

    public void Dispose() => _conn.Dispose();
}
