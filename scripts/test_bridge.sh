#!/bin/bash
# Test script to verify Rust bridge integration

cd "$(dirname "$0")"

echo "Testing Rust bridge integration..."
echo ""

# Test 1: Check if Rust library exists
echo "1. Checking Rust library..."
if [ -f "target/release/libelsewhen.so" ]; then
    echo "   ✓ libelsewhen.so found"
else
    echo "   ✗ libelsewhen.so not found"
    exit 1
fi

# Test 2: Check if generated bridge code exists
echo ""
echo "2. Checking generated bridge code..."
if [ -f "ui/lib/bridge/generated.dart/api.dart" ]; then
    echo "   ✓ api.dart generated"
else
    echo "   ✗ api.dart not generated"
    exit 1
fi

if [ -f "ui/lib/bridge/generated.dart/frb_generated.dart" ]; then
    echo "   ✓ frb_generated.dart generated"
else
    echo "   ✗ frb_generated.dart not generated"
    exit 1
fi

# Test 3: Check if database exists
echo ""
echo "3. Checking database..."
DB_PATH="$HOME/.local/share/elsewhen/elsewhen.db"
if [ -f "$DB_PATH" ]; then
    echo "   ✓ Database exists at $DB_PATH"

    # Check tables
    TABLES=$(sqlite3 "$DB_PATH" ".tables" 2>&1)
    echo "   Tables: $TABLES"

    # Check event count
    COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM events;" 2>&1)
    echo "   Events count: $COUNT"
else
    echo "   ✗ Database not found"
fi

# Test 4: Verify app processes
echo ""
echo "4. Checking running processes..."
PROCESS_COUNT=$(ps aux | grep elsewhen_ui | grep -v grep | wc -l)
echo "   Running instances: $PROCESS_COUNT"

echo ""
echo "Bridge integration test complete!"
