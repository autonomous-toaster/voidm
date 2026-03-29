#!/bin/bash

################################################################################
# VOIDM SQLite → Neo4j Migration Script
# 
# This script migrates all memories from SQLite to Neo4j by:
# 1. Exporting all memories from SQLite as JSON
# 2. Switching config backend to Neo4j
# 3. Re-importing each memory using "voidm add" with full metadata
# 4. Validating the migration succeeded
#
# Usage: ./migrate.sh [--dry-run] [--continue-on-error]
################################################################################

set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script configuration
DRY_RUN=false
CONTINUE_ON_ERROR=false
VOIDM_BIN=""  # Set by check_voidm()
BACKUP_DIR="$HOME/voidm_migration_backup"
EXPORT_FILE="/tmp/voidm_export_$(date +%s).json"
LOG_FILE="voidm_migration_$(date +%Y%m%d_%H%M%S).log"
FAILED_COUNT=0
SUCCESS_COUNT=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --continue-on-error)
            CONTINUE_ON_ERROR=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

################################################################################
# Helper Functions
################################################################################

log() {
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${BLUE}[${timestamp}]${NC} $1" | tee -a "$LOG_FILE"
}

info() {
    echo -e "${GREEN}✓${NC} $1" | tee -a "$LOG_FILE"
}

warn() {
    echo -e "${YELLOW}⚠${NC} $1" | tee -a "$LOG_FILE"
}

error() {
    echo -e "${RED}✗${NC} $1" | tee -a "$LOG_FILE"
}

die() {
    error "$1"
    exit 1
}

confirm() {
    if [ "$DRY_RUN" = true ]; then
        return 0
    fi
    
    local prompt="$1"
    local response
    read -p "$(echo -e ${YELLOW})$prompt${NC} (y/N): " response
    [[ "$response" =~ ^[Yy]$ ]]
}

################################################################################
# Pre-Migration Checks
################################################################################

check_voidm() {
    log "Checking voidm installation..."
    
    # Try release binary first
    if [ -x ./target/release/voidm ]; then
        VOIDM_BIN="./target/release/voidm"
        info "voidm found (release binary): $VOIDM_BIN"
        return 0
    fi
    
    # Fall back to PATH
    if command -v voidm &> /dev/null; then
        VOIDM_BIN="voidm"
        info "voidm found (PATH): $(command -v voidm)"
        return 0
    fi
    
    die "voidm not found - neither ./target/release/voidm nor PATH contains voidm"
}

check_sqlite() {
    log "Checking SQLite database..."
    local db_path="$HOME/Library/Application Support/voidm/memories.db"
    
    if [ ! -f "$db_path" ]; then
        die "SQLite database not found at: $db_path"
    fi
    info "SQLite database found: $db_path"
    
    # Count memories
    local count=$(sqlite3 "$db_path" "SELECT COUNT(*) FROM memories" 2>/dev/null || echo "0")
    info "Current memories in SQLite: $count"
    echo "$count"
}

get_memory_count() {
    local db_path="$HOME/Library/Application Support/voidm/memories.db"
    sqlite3 "$db_path" "SELECT COUNT(*) FROM memories" 2>/dev/null || echo "0"
}

################################################################################
# Export Phase
################################################################################

export_memories() {
    log "Exporting all memories from SQLite..."
    
    # Use --limit 10000 to ensure we get all memories (we have ~1070)
    if ! $VOIDM_BIN export --json --quiet --limit 10000 -o "$EXPORT_FILE" 2>/dev/null; then
        die "Failed to export memories"
    fi
    
    if [ ! -f "$EXPORT_FILE" ]; then
        die "Export file was not created"
    fi
    
    local file_size=$(du -h "$EXPORT_FILE" | cut -f1)
    info "Memories exported to: $EXPORT_FILE ($file_size)"
}

count_exported_memories() {
    # Count memories in the JSON export
    grep -o '"id":' "$EXPORT_FILE" | wc -l
}

################################################################################
# Configuration Phase
################################################################################

backup_config() {
    log "Backing up current configuration..."
    
    local config_path="$HOME/.config/voidm/config.toml"
    if [ ! -f "$config_path" ]; then
        die "Config file not found at: $config_path"
    fi
    
    local backup_file="${config_path}.backup.$(date +%s)"
    cp "$config_path" "$backup_file"
    info "Config backed up to: $backup_file"
}

backup_sqlite() {
    log "Creating SQLite database backup..."
    
    mkdir -p "$BACKUP_DIR"
    local db_path="$HOME/Library/Application Support/voidm/memories.db"
    local backup_db="${BACKUP_DIR}/memories.db.backup.$(date +%Y%m%d_%H%M%S)"
    
    cp "$db_path" "$backup_db"
    info "SQLite backed up to: $backup_db"
}

switch_to_neo4j() {
    log "Switching backend configuration to Neo4j..."
    
    if [ "$DRY_RUN" = true ]; then
        warn "[DRY RUN] Would switch backend to Neo4j"
        return 0
    fi
    
    local config_path="$HOME/.config/voidm/config.toml"
    
    # Replace backend = "sqlite" with backend = "neo4j"
    if sed -i.bak 's/backend = "sqlite"/backend = "neo4j"/' "$config_path"; then
        info "Backend switched to Neo4j"
    else
        die "Failed to update config"
    fi
    
    # Verify change
    local current_backend=$(grep 'backend = ' "$config_path" | head -1 | cut -d'"' -f2)
    if [ "$current_backend" != "neo4j" ]; then
        die "Config update verification failed (backend is still: $current_backend)"
    fi
}

verify_neo4j_connection() {
    log "Verifying Neo4j connection..."
    
    if ! $VOIDM_BIN search "test" --limit 1 &>/dev/null; then
        die "Neo4j connection failed - cannot proceed"
    fi
    
    info "Neo4j connection verified"
}

################################################################################
# Import Phase
################################################################################

import_memories() {
    log "Re-importing memories into Neo4j..."
    info "This will take several minutes..."
    
    local total_memories=$(count_exported_memories)
    local current=0
    
    # Parse JSON and extract each memory
    # Using jq to parse JSON safely
    if ! command -v jq &> /dev/null; then
        die "jq not found - required for JSON parsing"
    fi
    
    # Extract memories array and iterate
    jq -c '.memories[]' "$EXPORT_FILE" | while IFS= read -r memory; do
        current=$((current + 1))
        
        # Extract fields
        local id=$(echo "$memory" | jq -r '.id')
        local type=$(echo "$memory" | jq -r '.type')
        local content=$(echo "$memory" | jq -r '.content')
        local importance=$(echo "$memory" | jq -r '.importance // 5')
        local tags=$(echo "$memory" | jq -r '.tags[]?' | paste -sd ',' -)
        local scopes=$(echo "$memory" | jq -r '.scopes[]?' | xargs)
        local title=$(echo "$memory" | jq -r '.title // empty')
        local context=$(echo "$memory" | jq -r '.context // empty')
        
        # Build voidm add command
        local cmd="$VOIDM_BIN add --type '$type' --quiet --json"
        
        # Add importance if not default
        if [ "$importance" != "5" ]; then
            cmd="$cmd --importance '$importance'"
        fi
        
        # Add tags if present
        if [ -n "$tags" ]; then
            cmd="$cmd --tags '$tags'"
        fi
        
        # Add scopes if present
        if [ -n "$scopes" ]; then
            while read -r scope; do
                cmd="$cmd --scope '$scope'"
            done <<< "$scopes"
        fi
        
        # Add title if present
        if [ -n "$title" ] && [ "$title" != "null" ]; then
            cmd="$cmd --title '$title'"
        fi
        
        # Add context if present
        if [ -n "$context" ] && [ "$context" != "null" ]; then
            cmd="$cmd --context '$context'"
        fi
        
        # Add content (must be last)
        cmd="$cmd '$content'"
        
        # Execute command
        if [ "$DRY_RUN" = true ]; then
            info "[DRY RUN] Would import ($current/$total_memories): $id"
        else
            # if eval "$cmd" &>/dev/null; then
            if eval "$cmd"; then
                # echo -n "."
                log "Imported memory: $id - $title"
                SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
            else
                error "Failed to import memory: $id"
                FAILED_COUNT=$((FAILED_COUNT + 1))
                if [ "$CONTINUE_ON_ERROR" != true ]; then
                    die "Migration aborted at memory $current/$total_memories"
                fi
            fi
        fi
        
        # Progress indicator
        if [ $((current % 50)) -eq 0 ]; then
            echo ""
            log "Progress: $current/$total_memories"
        fi
    done
    
    echo "" # newline after progress dots
}

################################################################################
# Verification Phase
################################################################################

verify_migration() {
    log "Verifying migration..."
    
    if [ "$DRY_RUN" = true ]; then
        info "[DRY RUN] Would verify counts"
        return 0
    fi
    
    local exported=$(count_exported_memories)
    local imported=$SUCCESS_COUNT
    
    info "Exported memories: $exported"
    info "Successfully imported: $imported"
    info "Failed imports: $FAILED_COUNT"
    
    local expected=$exported
    local tolerance=$((expected / 20))  # 5% tolerance
    
    if [ $imported -ge $((expected - tolerance)) ] && [ $imported -le $((expected + tolerance)) ]; then
        info "Memory count verification: PASSED ✓"
        return 0
    else
        error "Memory count verification: FAILED"
        error "Expected: $expected, Got: $imported (tolerance: ±$tolerance)"
        return 1
    fi
}

test_neo4j_functionality() {
    log "Testing Neo4j functionality..."
    
    if [ "$DRY_RUN" = true ]; then
        info "[DRY RUN] Would test Neo4j"
        return 0
    fi
    
    # Test search
    if voidm search "test" --limit 1 &>/dev/null; then
        info "Search functionality: OK ✓"
    else
        error "Search functionality: FAILED"
        return 1
    fi
    
    # Test stats
    if voidm stats &>/dev/null; then
        info "Stats functionality: OK ✓"
    else
        error "Stats functionality: FAILED"
        return 1
    fi
    
    return 0
}

################################################################################
# Cleanup Phase
################################################################################

cleanup() {
    log "Cleaning up temporary files..."
    
    if [ -f "$EXPORT_FILE" ]; then
        rm -f "$EXPORT_FILE"
        info "Removed export file: $EXPORT_FILE"
    fi
}

show_summary() {
    log "Migration Summary:"
    echo ""
    echo "Status: $([ $FAILED_COUNT -eq 0 ] && echo -e "${GREEN}SUCCESS${NC}" || echo -e "${YELLOW}PARTIAL${NC}")"
    echo "Exported: $(count_exported_memories)"
    echo "Imported: $SUCCESS_COUNT"
    echo "Failed: $FAILED_COUNT"
    echo ""
    echo "Log: $LOG_FILE"
    echo ""
    
    if [ $FAILED_COUNT -eq 0 ]; then
        info "Migration completed successfully!"
        return 0
    else
        warn "Migration completed with $FAILED_COUNT failures"
        warn "Review $LOG_FILE for details"
        return 1
    fi
}

################################################################################
# Rollback Functions
################################################################################

rollback() {
    error "Rolling back changes..."
    
    log "Restoring SQLite as primary backend..."
    local config_path="$HOME/.config/voidm/config.toml"
    sed -i.bak 's/backend = "neo4j"/backend = "sqlite"/' "$config_path"
    info "Backend restored to SQLite"
    
    warn "Manual steps required:"
    warn "1. Manually restore SQLite backup if data was lost"
    warn "2. Review migration log: $LOG_FILE"
    warn "3. Contact support if needed"
}

################################################################################
# Main Migration Flow
################################################################################

main() {
    log "======================================================"
    log "VOIDM SQLite → Neo4j Migration"
    log "======================================================"
    echo ""
    
    if [ "$DRY_RUN" = true ]; then
        warn "DRY RUN MODE - No changes will be made"
        echo ""
    fi
    
    # Pre-migration checks
    log "PHASE 1: Pre-Migration Checks"
    check_voidm
    sqlite_count=$(check_sqlite)
    echo ""
    
    # Backups
    log "PHASE 2: Creating Backups"
    backup_config
    backup_sqlite
    echo ""
    
    # Export
    log "PHASE 3: Exporting from SQLite"
    export_memories
    export_count=$(count_exported_memories)
    info "Exported: $export_count memories"
    echo ""
    
    # Configuration
    log "PHASE 4: Switching Configuration"
    
    if ! confirm "Switch backend to Neo4j?"; then
        warn "Migration cancelled by user"
        exit 0
    fi
    
    switch_to_neo4j
    
    if [ "$DRY_RUN" != true ]; then
        sleep 2  # Give system time to process config change
        verify_neo4j_connection
    fi
    echo ""
    
    # Import
    log "PHASE 5: Importing into Neo4j"
    import_memories
    echo ""
    
    # Verification
    log "PHASE 6: Verification"
    if verify_migration && test_neo4j_functionality; then
        info "All verifications passed ✓"
    else
        error "Some verifications failed"
        if ! confirm "Continue anyway?"; then
            rollback
            die "Migration aborted by user"
        fi
    fi
    echo ""
    
    # Cleanup
    cleanup
    echo ""
    
    # Summary
    show_summary
    
    log "======================================================"
    log "Migration complete!"
    log "======================================================"
}

# Trap errors and cleanup
trap cleanup EXIT

# Run main migration flow
main "$@"
