using System.Globalization;

namespace Twi;

internal static class Program
{
    [STAThread]
    private static void Main()
    {
        CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo("ru-RU");
        ApplicationConfiguration.Initialize();
        Application.Run(new MainForm());
    }
}
