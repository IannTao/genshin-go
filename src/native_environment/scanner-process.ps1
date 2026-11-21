# Runs in the same Windows integrity level as the scanner so it can request WM_CLOSE.
$ErrorActionPreference = 'Stop'
$connection = [Net.Sockets.TcpClient]::new()
$scanner = $null
try {
    $connection.Connect([Net.IPAddress]::Loopback, $port)
    $stream = $connection.GetStream()
    $stream.ReadTimeout = 5000
    $reader = [IO.StreamReader]::new($stream)
    if ($reader.ReadLine() -cne $token) { return }

    $program = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($program64))
    $info = [Diagnostics.ProcessStartInfo]::new($program, '--agent-mode')
    $info.UseShellExecute = $false
    $info.WorkingDirectory = [IO.Path]::GetDirectoryName($program)
    try {
        $scanner = [Diagnostics.Process]::Start($info)
    } catch {
        $stream.WriteByte(0)
        return
    }
    $stream.WriteByte(1)
    while (!$scanner.HasExited -and !$connection.Client.Poll(250000, [Net.Sockets.SelectMode]::SelectRead)) {}
} finally {
    $connection.Dispose()
    if ($null -ne $scanner) {
        # A skip during startup can arrive before the scanner creates its main window.
        try {
            for ($attempt = 0; $attempt -lt 40 -and !$scanner.HasExited; $attempt++) {
                $scanner.Refresh()
                if ($scanner.CloseMainWindow()) { break }
                Start-Sleep -Milliseconds 250
            }
        } finally {
            $scanner.Dispose()
        }
    }
}
