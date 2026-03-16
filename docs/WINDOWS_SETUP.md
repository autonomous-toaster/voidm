# Windows Configuration & Setup

## Configuration File Locations

### Windows Paths

voidm follows platform conventions for configuration file locations using the `dirs` crate:

#### Configuration Directory (config.toml)
- **Primary**: `%APPDATA%\voidm\config.toml` (e.g., `C:\Users\YourName\AppData\Roaming\voidm\config.toml`)
- **Fallback**: `%USERPROFILE%\.config\voidm\config.toml`
- **Override**: Set `VOIDM_CONFIG_HOME` environment variable to use custom location

#### Database Directory (memories.db)
- **Default**: `%LOCALAPPDATA%\voidm\memories.db` (e.g., `C:\Users\YourName\AppData\Local\voidm\memories.db`)
- **Override**: Use `--db` flag or `VOIDM_DB` environment variable

#### Model Cache Directory
- **Default**: `%LOCALAPPDATA%\voidm\models\` (for embeddings and LLM models)
- Automatically created by voidm on first use

### Configuration Path Resolution

voidm searches for `config.toml` in this order:

1. **Environment Variable**: `VOIDM_CONFIG_HOME` (if set and not empty)
2. **AppData**: `%APPDATA%\voidm\config.toml` (Windows standard)
3. **Home Directory**: `%USERPROFILE%\.config\voidm\config.toml` (fallback)

If no config file exists, voidm uses built-in defaults.

### Database Path Resolution

Database path is resolved in this order:

1. **CLI Flag**: `voidm search --db "C:\path\to\memories.db"`
2. **Environment Variable**: `VOIDM_DB="C:\Users\YourName\AppData\Local\voidm\memories.db"`
3. **Config File**: `[database.sqlite]` section in `config.toml`
4. **Default**: `%LOCALAPPDATA%\voidm\memories.db`

## Creating a Config File on Windows

### Method 1: Manual Creation

Create `%APPDATA%\voidm\config.toml` (you may need to create the directory first):

```powershell
# PowerShell
$configDir = "$env:APPDATA\voidm"
if (-not (Test-Path $configDir)) {
    New-Item -ItemType Directory -Path $configDir
}

# Create config file (replace with your settings)
@"
[database]
backend = "sqlite"

[database.sqlite]
path = "%LOCALAPPDATA%\voidm\memories.db"

[embeddings]
enabled = true
model = "Xenova/all-MiniLM-L6-v2"

[search]
mode = "hybrid"
default_limit = 10
min_score = 0.5
"@ | Out-File -Encoding UTF8 "$configDir\config.toml"
```

### Method 2: Using voidm Command

```powershell
voidm config init
# Prompts for configuration interactively
```

### Method 3: Environment Variables

Set environment variables instead of creating a config file:

```powershell
# Set in PowerShell
$env:VOIDM_DATABASE_BACKEND = "sqlite"
$env:VOIDM_DATABASE_SQLITE_PATH = "C:\Users\YourName\AppData\Local\voidm\memories.db"
$env:VOIDM_EMBEDDINGS_ENABLED = "true"
$env:VOIDM_EMBEDDINGS_MODEL = "Xenova/all-MiniLM-L6-v2"

# Or set permanently (requires admin)
[System.Environment]::SetEnvironmentVariable("VOIDM_DATABASE_BACKEND", "sqlite", [System.EnvironmentVariableTarget]::User)
```

## Windows-Specific Considerations

### File Paths in Config Files

When specifying paths in `config.toml` on Windows:

✅ **Forward slashes** (preferred, platform-independent):
```toml
[database.sqlite]
path = "C:/Users/YourName/AppData/Local/voidm/memories.db"
```

✅ **Tilde expansion** (home directory):
```toml
[database.sqlite]
path = "~/AppData/Local/voidm/memories.db"
```

❌ **Raw backslashes** (requires escaping in TOML):
```toml
# This WON'T work - backslashes need escaping
path = "C:\Users\...\memories.db"

# This WILL work - doubled backslashes
path = "C:\\Users\\YourName\\AppData\\Local\\voidm\\memories.db"
```

### Environment Variables in PowerShell

When using environment variables, paths must follow PowerShell syntax:

```powershell
# Correct
$env:VOIDM_DATABASE_SQLITE_PATH = "C:\Users\YourName\AppData\Local\voidm\memories.db"

# From batch script, use forward slashes or escape
set VOIDM_DATABASE_SQLITE_PATH=C:/Users/YourName/AppData/Local/voidm/memories.db
```

### Database File Permissions

Ensure the voidm directory has write permissions:

```powershell
# Check permissions
icacls C:\Users\YourName\AppData\Local\voidm

# Grant permissions if needed (assumes admin)
icacls C:\Users\YourName\AppData\Local\voidm /grant:r "$env:USERNAME:(F)"
```

## ONNX Model Cache on Windows

By default, embeddings models are cached in:
```
%LOCALAPPDATA%\voidm\models\
```

To use a custom model cache directory:

```toml
# config.toml
[embeddings]
enabled = true
model = "Xenova/all-MiniLM-L6-v2"
cache_dir = "C:/models/voidm"
```

Or via environment variable:
```powershell
$env:VOIDM_EMBEDDINGS_CACHE_DIR = "C:\models\voidm"
```

## Antivirus and Real-Time Protection

If you experience slow builds or database operations on Windows:

1. **Exclude voidm directory** from real-time scanning:
   - Add `%LOCALAPPDATA%\voidm\` to antivirus exclusions
   - Add voidm binary to exclusions

2. **Exclude source directory** from scanning (during development):
   - Add your project folder to exclusions
   - Restore after development

3. **Disable real-time protection** during initial setup:
   ```powershell
   # View status
   Get-MpPreference | Select RealTimeProtectionEnabled
   
   # Temporarily disable (requires admin)
   Set-MpPreference -DisableRealtimeMonitoring $true
   ```

## Troubleshooting

### Config File Not Found

If voidm shows "using defaults", check:

```powershell
# Check what voidm expects
voidm info

# Manually verify path
Test-Path "$env:APPDATA\voidm\config.toml"
Test-Path "$env:LOCALAPPDATA\voidm\memories.db"
```

### Database Lock Issues

If you get "database is locked" errors:

1. Ensure only one voidm instance is running
2. Check Task Manager for ghost processes
3. Check antivirus is not locking the file

```powershell
# Kill any running voidm processes
Get-Process voidm -ErrorAction SilentlyContinue | Stop-Process -Force
```

### Path Resolution Issues

Test path resolution with:

```powershell
# Show resolved config path
voidm config show

# Override with explicit path
voidm search --db "C:\my\custom\path.db" "your query"

# Or use environment variable
$env:VOIDM_DB = "C:\my\custom\path.db"
voidm search "your query"
```

## Performance Tips

1. **Use local SSD**: Store `memories.db` on fast local drive, not network share
2. **Exclude from indexing**: Right-click folder → Properties → Exclude from indexing
3. **Disable sync temporarily**: Pause OneDrive/Dropbox sync during large imports
4. **Check antivirus**: Real-time scanning can significantly slow SQLite operations

## Additional Resources

- [Rust on Windows Guide](https://rust-lang.org/what/wg-windows/)
- [Windows Developer Setup](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup)
- [Cargo Documentation](https://doc.rust-lang.org/cargo/)
