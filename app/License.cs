using System.Globalization;
using System.Security.Cryptography;
using System.Text;

namespace Twi;

internal static class License
{
    private const string Product = "TWI1";
    private const long Day = 86400;
    private const long MoscowOffset = 10800;
    internal static readonly string DataDir = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "TwoWindowsInOne");

    private static readonly string KeyPath = Path.Combine(DataDir, "license.txt");

    internal static string Load()
    {
        try
        {
            return File.Exists(KeyPath) ? File.ReadAllText(KeyPath) : "";
        }
        catch (IOException) { return ""; }
        catch (UnauthorizedAccessException) { return ""; }
    }

    internal static void Save(string key)
    {
        Directory.CreateDirectory(DataDir);
        var temporaryPath = Path.Combine(DataDir, $"license-{Guid.NewGuid():N}.tmp");
        try
        {
            File.WriteAllText(temporaryPath, key.Trim());
            File.Move(temporaryPath, KeyPath, overwrite: true);
        }
        finally
        {
            File.Delete(temporaryPath);
        }
    }

    internal static bool Verify(string key, DateTimeOffset now, out DateTimeOffset expiry, out string error)
    {
        expiry = default;
        error = "Ключ не распознан. Скопируйте его целиком с сайта.";

        if (key.Length > 512)
            return false;

        string[] parts = key.Trim().Split('.');
        if (parts.Length != 4 || parts[0] != Product ||
            !long.TryParse(parts[1], NumberStyles.None, CultureInfo.InvariantCulture, out long starts) ||
            !long.TryParse(parts[2], NumberStyles.None, CultureInfo.InvariantCulture, out long ends) ||
            ends < 0 || ends > 253402300799 || ends - starts != Day ||
            starts % Day != Day - MoscowOffset ||
            parts[3].Length != 342)
            return false;

        using Stream? publicKey = typeof(License).Assembly.GetManifestResourceStream("Twi.LicenseKey");
        if (publicKey is null)
        {
            error = "В этой сборке не настроена проверка ключей.";
            return false;
        }

        try
        {
            using var reader = new StreamReader(publicKey);
            using var rsa = RSA.Create();
            rsa.ImportFromPem(reader.ReadToEnd());
            byte[] payload = Encoding.ASCII.GetBytes(string.Join(".", parts, 0, 3));
            byte[] signature = Convert.FromBase64String(parts[3].Replace('-', '+').Replace('_', '/') + "==");
            if (!rsa.VerifyData(payload, signature, HashAlgorithmName.SHA256, RSASignaturePadding.Pkcs1))
                return false;
        }
        catch (FormatException) { return false; }
        catch (CryptographicException) { return false; }

        long current = now.ToUnixTimeSeconds();
        if (current < starts)
        {
            error = "Время действия ключа ещё не началось. Проверьте системные часы.";
            return false;
        }

        if (current >= ends)
        {
            error = "Срок ключа истёк. Получите новый ключ на сайте.";
            return false;
        }

        expiry = DateTimeOffset.FromUnixTimeSeconds(ends).ToOffset(TimeSpan.FromHours(3));
        error = "";
        return true;
    }
}
