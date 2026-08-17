using System.Reflection;

namespace Hyperkey.App;

internal static class AppVersion
{
    public static string Display { get; } = ResolveDisplayVersion();

    private static string ResolveDisplayVersion()
    {
        var assembly = Assembly.GetEntryAssembly() ?? typeof(AppVersion).Assembly;
        var informationalVersion = assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()
            ?.InformationalVersion;

        if (!string.IsNullOrWhiteSpace(informationalVersion))
        {
            return informationalVersion.Split('+', 2)[0];
        }

        var version = assembly.GetName().Version;
        return version is null
            ? "Unknown"
            : $"{version.Major}.{version.Minor}.{Math.Max(version.Build, 0)}";
    }
}
