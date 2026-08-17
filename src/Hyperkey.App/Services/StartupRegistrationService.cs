using System.IO;
using System.Security;
using Microsoft.Win32;

namespace Hyperkey.App.Services;

public sealed record StartupRegistrationResult(bool Succeeded, string? Error)
{
    public static StartupRegistrationResult Success { get; } = new(true, null);

    public static StartupRegistrationResult Failure(string error) => new(false, error);
}

public sealed class StartupRegistrationService
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "Hyperkey";

    public StartupRegistrationResult Apply(bool enabled)
    {
        try
        {
            if (!enabled)
            {
                using var existingKey = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
                existingKey?.DeleteValue(ValueName, throwOnMissingValue: false);
                return StartupRegistrationResult.Success;
            }

            var executablePath = Environment.ProcessPath;
            if (string.IsNullOrWhiteSpace(executablePath))
            {
                return StartupRegistrationResult.Failure(
                    "Launch at login could not be updated because the Hyperkey executable path is unavailable.");
            }

            using var runKey = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true);
            if (runKey is null)
            {
                return StartupRegistrationResult.Failure(
                    "Launch at login could not be updated because Windows startup settings could not be opened.");
            }

            runKey.SetValue(ValueName, QuoteExecutablePath(executablePath), RegistryValueKind.String);
            return StartupRegistrationResult.Success;
        }
        catch (Exception exception) when (exception is SecurityException
            or UnauthorizedAccessException
            or IOException
            or InvalidOperationException)
        {
            return StartupRegistrationResult.Failure($"Launch at login could not be updated: {exception.Message}");
        }
    }

    private static string QuoteExecutablePath(string executablePath)
    {
        var escapedPath = executablePath.Replace("\"", "\\\"", StringComparison.Ordinal);
        return $"\"{escapedPath}\"";
    }
}
