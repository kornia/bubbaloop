#!/bin/bash
# Create a new bubbaloop node from template
#
# Usage: new-node.sh <name> [path] [description]
#
# Examples:
#   new-node.sh my-sensor                           # Creates in ~/.bubbaloop/nodes/my-sensor
#   new-node.sh my-sensor ~/projects/my-sensor      # Creates in specified path
#   new-node.sh my-sensor . "My sensor node"        # With description

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get script directory (where templates are)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATE_DIR="$SCRIPT_DIR/../templates/rust-node"
BUBBALOOP_NODES_DIR="$HOME/.bubbaloop/nodes"
NODES_FILE="$HOME/.bubbaloop/nodes.json"

# Parse arguments
NAME="$1"
TARGET_PATH="$2"
DESCRIPTION="${3:-A bubbaloop node}"

if [ -z "$NAME" ]; then
    echo -e "${RED}Error: Node name is required${NC}"
    echo ""
    echo "Usage: $0 <name> [path] [description]"
    echo ""
    echo "Examples:"
    echo "  $0 my-sensor                           # Creates in ~/.bubbaloop/nodes/my-sensor"
    echo "  $0 my-sensor ~/projects/my-sensor      # Creates in specified path"
    echo "  $0 my-sensor . \"My sensor node\"        # With description"
    exit 1
fi

# Validate name (alphanumeric, hyphens, underscores)
if ! [[ "$NAME" =~ ^[a-zA-Z][a-zA-Z0-9_-]*$ ]]; then
    echo -e "${RED}Error: Invalid node name '$NAME'${NC}"
    echo "Name must start with a letter and contain only letters, numbers, hyphens, and underscores"
    exit 1
fi

# Determine target path
if [ -z "$TARGET_PATH" ]; then
    TARGET_PATH="$BUBBALOOP_NODES_DIR/$NAME"
elif [ "$TARGET_PATH" = "." ]; then
    TARGET_PATH="$(pwd)/$NAME"
elif [[ "$TARGET_PATH" != /* ]]; then
    TARGET_PATH="$(pwd)/$TARGET_PATH"
fi

# Check if already exists
if [ -d "$TARGET_PATH" ]; then
    echo -e "${RED}Error: Directory already exists: $TARGET_PATH${NC}"
    exit 1
fi

# Convert name to PascalCase for struct name
NODE_STRUCT=$(echo "$NAME" | sed -r 's/(^|[-_])([a-z])/\U\2/g')

# Get author from git config or default
AUTHOR=$(git config user.name 2>/dev/null || echo "Unknown")

echo -e "${BLUE}Creating new node: ${GREEN}$NAME${NC}"
echo -e "  Path: $TARGET_PATH"
echo -e "  Description: $DESCRIPTION"
echo ""

# Create directory structure
mkdir -p "$TARGET_PATH/src"

# Process templates
process_template() {
    local src="$1"
    local dst="$2"

    sed -e "s/{{node_name}}/$NAME/g" \
        -e "s/{{node_name_pascal}}/$NODE_STRUCT/g" \
        -e "s/{{description}}/$DESCRIPTION/g" \
        -e "s/{{author}}/$AUTHOR/g" \
        "$src" > "$dst"
}

echo -e "${YELLOW}Generating files...${NC}"

# Copy and process templates
process_template "$TEMPLATE_DIR/Cargo.toml.template" "$TARGET_PATH/Cargo.toml"
process_template "$TEMPLATE_DIR/src/main.rs.template" "$TARGET_PATH/src/main.rs"
process_template "$TEMPLATE_DIR/src/node.rs.template" "$TARGET_PATH/src/node.rs"
process_template "$TEMPLATE_DIR/node.yaml.template" "$TARGET_PATH/node.yaml"
process_template "$TEMPLATE_DIR/config.yaml.template" "$TARGET_PATH/config.yaml"
process_template "$TEMPLATE_DIR/.gitignore.template" "$TARGET_PATH/.gitignore"

echo -e "  ${GREEN}✓${NC} Cargo.toml"
echo -e "  ${GREEN}✓${NC} src/main.rs"
echo -e "  ${GREEN}✓${NC} src/node.rs"
echo -e "  ${GREEN}✓${NC} node.yaml"
echo -e "  ${GREEN}✓${NC} config.yaml"
echo -e "  ${GREEN}✓${NC} .gitignore"

# Register in nodes.json
echo ""
echo -e "${YELLOW}Registering node...${NC}"

mkdir -p "$(dirname "$NODES_FILE")"

TIMESTAMP=$(date -Iseconds)

if [ -f "$NODES_FILE" ]; then
    # Use Python to update JSON (more reliable than jq/sed)
    python3 << PYEOF
import json
with open("$NODES_FILE", "r") as f:
    data = json.load(f)
data["nodes"].append({"path": "$TARGET_PATH", "addedAt": "$TIMESTAMP"})
with open("$NODES_FILE", "w") as f:
    json.dump(data, f, indent=2)
PYEOF
else
    # Create new registry
    cat > "$NODES_FILE" << EOF
{
  "nodes": [
    {"path": "$TARGET_PATH", "addedAt": "$TIMESTAMP"}
  ]
}
EOF
fi

echo -e "  ${GREEN}✓${NC} Added to $NODES_FILE"

# Done
echo ""
echo -e "${GREEN}Node created successfully!${NC}"
echo ""
echo "Next steps:"
echo -e "  ${BLUE}1.${NC} cd $TARGET_PATH"
echo -e "  ${BLUE}2.${NC} Edit src/node.rs to implement your logic"
echo -e "  ${BLUE}3.${NC} Edit config.yaml to configure topics"
echo -e "  ${BLUE}4.${NC} cargo build --release"
echo -e "  ${BLUE}5.${NC} ./target/release/${NAME}_node"
echo ""
echo "To install as systemd service:"
echo -e "  ${BLUE}bubbaloop tui${NC} → /nodes → select node → [i]nstall"
