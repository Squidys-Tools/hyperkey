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
    private readonly string _backupSettingsPath;

    public SettingsStore()
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "Hyperkey"))
    {
    }

    public SettingsStore(string settingsDirectory)
    {
        _settingsPath = Path.Combine(settingsDirectory, "settings.json");
        _temporarySettingsPath = $"{_settingsPath}.tmp";
        _backupSettingsPath = $"{_settingsPath}.bak";
    }

    public SettingsLoadResult Load()
    {
        TryDeleteTemporaryFile(_temporarySettingsPath);

        if (!File.Exists(_settingsPath))
        {
            return new SettingsLoadResult(HyperkeySettings.Defaults, UsedDefaults: false, Message: null);
        }

        string json;
        try
        {
            json = File.ReadAllText(_settingsPath);
        }
        catch (IOException exception)
        {
            return RecoverUnreadableFile($"The settings file could not be read: {exception.Message}");
        }
        catch (UnauthorizedAccessException exception)
        {
            return RecoverUnreadableFile($"Hyperkey could not access its settings file: {exception.Message}");
        }

        var parseResult = SettingsJson.Parse(json);
        if (parseResult.UsedDefaults)
        {
            var reason = $"The settings file could not be read: {parseResult.Error}";
            if (!parseResult.FileCorrupt)
            {
                // A readable file (for example, one from a newer schema version)
                // stays in place so its values can still be recovered later.
                return new SettingsLoadResult(
                    HyperkeySettings.Defaults,
                    UsedDefaults: true,
                    Message: $"Hyperkey loaded safe defaults. {reason}");
            }

            return RecoverUnreadableFile(reason);
        }

        return new SettingsLoadResult(
            parseResult.Settings,
            UsedDefaults: false,
            Message: parseResult.Repaired
                ? $"Some settings were invalid and were reset to defaults ({parseResult.Error})."
                : null);
    }

    private SettingsLoadResult RecoverUnreadableFile(string reason)
    {
        var preserved = TryPreserveAsBackup();
        return new SettingsLoadResult(
            HyperkeySettings.Defaults,
            UsedDefaults: true,
            Message: preserved
                ? $"Hyperkey loaded safe defaults. {reason} The original file was preserved as settings.json.bak."
                : $"Hyperkey loaded safe defaults. {reason}");
    }

    private bool TryPreserveAsBackup()
    {
        try
        {
            File.Move(_settingsPath, _backupSettingsPath, overwrite: true);
            return true;
        }
        catch (IOException)
        {
            return false;
        }
        catch (UnauthorizedAccessException)
        {
            return false;
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
