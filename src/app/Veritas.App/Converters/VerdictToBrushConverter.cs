using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace Veritas.App.Converters;

public sealed class VerdictToBrushConverter : IValueConverter
{
    private static readonly IBrush Hoax = new SolidColorBrush(Color.Parse("#C81E2B"));
    private static readonly IBrush Valid = new SolidColorBrush(Color.Parse("#12855B"));
    private static readonly IBrush Misleading = new SolidColorBrush(Color.Parse("#B26A00"));
    private static readonly IBrush Buzzer = new SolidColorBrush(Color.Parse("#7C3AED"));
    private static readonly IBrush Neutral = new SolidColorBrush(Color.Parse("#5A6B85"));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value?.ToString()?.ToLowerInvariant() switch
        {
            "hoax" => Hoax,
            "valid" => Valid,
            "misleading" => Misleading,
            "buzzer" => Buzzer,
            _ => Neutral,
        };
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}
