#requires -Version 7.2
param(
    [string]$DatabasePath,

    [ValidatePattern('\A[1-9][0-9]{8,9}\z')]
    [string]$Uid
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw '此脚本需要 Windows 自带的 winsqlite3.dll' }
if (-not $DatabasePath) {
    if (-not $env:LOCALAPPDATA) { throw '请通过 -DatabasePath 指定数据库路径' }
    $DatabasePath = Join-Path $env:LOCALAPPDATA 'Genshin-Go/launcher.sqlite3'
}
$database = Get-Item -LiteralPath $DatabasePath
if ($database.PSIsContainer) { throw '请指定 SQLite 数据库文件' }

if (-not ('GenshinGo.SqliteInspection' -as [type])) {
    Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace GenshinGo {
    public sealed class SqliteInspection : IDisposable {
        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_open_v2(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string path, out IntPtr db, int flags, IntPtr vfs);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_close(IntPtr db);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate int Callback(IntPtr context, int count, IntPtr values, IntPtr names);

        [DllImport("winsqlite3.dll", CallingConvention = CallingConvention.Cdecl)]
        private static extern int sqlite3_exec(IntPtr db,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string sql, Callback callback, IntPtr context, IntPtr error);

        private const int OpenReadOnly = 0x01;
        private IntPtr db;

        public SqliteInspection(string path) {
            int status = sqlite3_open_v2(path, out db, OpenReadOnly, IntPtr.Zero);
            if (status != 0) {
                Dispose();
                throw new InvalidOperationException("SQLite open failed (" + status + ")");
            }
        }

        public string[][] Read(string sql) {
            var rows = new List<string[]>();
            int status = sqlite3_exec(db, sql, (context, count, values, names) => {
                var row = new string[count];
                for (int i = 0; i < count; i++) {
                    row[i] = Marshal.PtrToStringUTF8(Marshal.ReadIntPtr(values, i * IntPtr.Size));
                }
                rows.Add(row);
                return 0;
            }, IntPtr.Zero, IntPtr.Zero);
            if (status != 0) throw new InvalidOperationException("SQLite read failed (" + status + ")");
            return rows.ToArray();
        }

        public void Dispose() {
            if (db != IntPtr.Zero) {
                sqlite3_close(db);
                db = IntPtr.Zero;
            }
        }
    }
}
'@
}

$reader = [GenshinGo.SqliteInspection]::new($database.FullName)
try {
    $rows = $reader.Read(@'
BEGIN;
PRAGMA user_version;
SELECT schema.name, info.name
FROM sqlite_master AS schema
JOIN pragma_table_info(schema.name) AS info
WHERE schema.type = 'table' AND schema.name NOT GLOB 'sqlite_*'
ORDER BY schema.name, info.cid;
'@)
    $tables = [ordered]@{}
    for ($index = 1; $index -lt $rows.Length; $index++) {
        $row = $rows[$index]
        $tables[$row[0]] += @($row[1])
    }
    $result = [ordered]@{
        UserVersion = [int]$rows[0][0]
        TableColumns = $tables
    }
    if ($Uid) {
        $result.UidSpaceExists = $false
        if ($tables['uid_spaces'] -ccontains 'uid') {
            # ValidatePattern restricts Uid to ASCII decimal digits before it reaches SQL.
            $match = $reader.Read("SELECT EXISTS(SELECT 1 FROM uid_spaces WHERE uid = '$Uid');")
            $result.UidSpaceExists = $match[0][0] -eq '1'
        }
    }
    [pscustomobject]$result
} finally {
    $reader.Dispose()
}
