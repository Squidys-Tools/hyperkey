using System.IO;
using Hyperkey.Core;

namespace Hyperkey.App.Services;

public sealed record SettingsLoadResult(
    HyperkeySettings Settings,
    bool UsedDefaults,
    string? Message);

public sealed record SettingsSaveResult(
    bool Succeeded,
    string? Error);

public sealed class SettingsStore
{
    private readonly string _settingsPath;
    private readonly string _temporarySettingsPath;

    public SettingsStore()
    {
        var appDataDirectory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Hyperkey");
        _settingsPath = Path.Combine(appDataDirectory, "settings.json");
        _temporarySettingsPath = $"{_settingsPath}.tmp";
    }

    public SettingsLoadResult Load()
    {
        TryDeleteTemporaryFile(_temporarySettingsPath);

        if (!File.Exists(_settingsPath))
        {
            return new SettingsLoadResult(HyperkeySettings.Defaults, UsedDefaults: false, Message: null);
        }

        try
        {
            var json = File.ReadAllText(_settingsPath);
            var parseResult = SettingsJson.Parse(json);
            return new SettingsLoadResult(
                parseResult.Settings,
                parseResult.UsedDefaults,
                parseResult.UsedDefaults
                    ? $"The settings file could not be read: {parseResult.Error}"
                    : null);
        }
        catch (IOException exception)
        {
            return new SettingsLoadResult(
                HyperkeySettings.Defaults,
                UsedDefaults: true,
                Message: $"The settings file could not be read: {exception.Message}");
        }
        catch (UnauthorizedAccessException exception)
        {
            return new SettingsLoadResult(
                HyperkeySettings.Defaults,
                UsedDefaults: true,
                Message: $"Hyperkey could not access its settings file: {exception.Message}");
        }
    }

    public SettingsSaveResult Save(HyperkeySettings settings)
    {
        try
        {
            var directory = Path.GetDirectoryName(_settingsPath)
                ?? throw new InvalidOperationException("The settings directory could not be resolved.");
            Directory.CreateDirectory(directory);

            File.WriteAllText(_temporarySettingsPath, SettingsJson.Serialize(settings));
            File.Move(_temporarySettingsPath, _settingsPath, overwrite: true);
            return new SettingsSaveResult(Succeeded: true, Error: null);
        }
        catch (IOException exception)
        {
            TryDeleteTemporaryFile(_temporarySettingsPath);
            return new SettingsSaveResult(Succeeded: false, Error: $"Settings could not be saved: {exception.Message}");
        }
        catch (Exception exception) when (exception is UnauthorizedAccessException
            or InvalidOperationException
            or ArgumentException
            or NotSupportedException)
        {
            TryDeleteTemporaryFile(_temporarySettingsPath);
            return new SettingsSaveResult(Succeeded: false, Error: $"Hyperkey could not write its settings: {exception.Message}");
        }
    }

    private static void TryDeleteTemporaryFile(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }
}
