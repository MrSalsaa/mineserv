#!/bin/bash

# ==============================================================================
# Mineserv Professional Intelligent Installer
# ==============================================================================
# Supports: Ubuntu, Debian, Arch, Fedora, Oracle Linux 8+, RHEL, CentOS
# Features: Intelligent dependency detection, Auto-fix logic, Interactive Onboarding
# ==============================================================================

set -e

# Colors for professional output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}======================================================"
echo -e "    MINESERV - PROFESSIONAL INTELLIGENT INSTALLER"
echo -e "======================================================${NC}"

# Ensure we have sudo/root for system-level changes
check_root() {
    if [ "$EUID" -ne 0 ]; then
        echo -e "${YELLOW}Notice: System-level changes (tweaks, services) require root.${NC}"
        echo -e "${YELLOW}Please run with: sudo bash $0${NC}"
        # We don't exit immediately, but flag for later
        IS_ROOT=false
    else
        IS_ROOT=true
    fi
}

# Distro Detection
detect_os() {
    if [ -f /etc/os-release ]; then
        . /etc/os-release
        OS_ID=$ID
        OS_VERSION=$VERSION_ID
    else
        echo -e "${RED}Error: Cannot detect Linux distribution.${NC}"
        exit 1
    fi
    echo -e "${GREEN}Detected OS: $NAME ($VERSION)${NC}"
}

# Intelligent Package Installation with Auto-Fix
install_pkg() {
    local cmd=""
    case $OS_ID in
        ubuntu|debian|kali|raspbian)
            cmd="apt-get install -y"
            # Update cache if needed
            sudo apt-get update -qq || echo -e "${YELLOW}Warning: apt update failed, attempting install anyway.${NC}"
            ;;
        arch|manjaro)
            cmd="pacman -Syu --noconfirm"
            ;;
        fedora|ol|rhel|centos|almalinux|rocky)
            cmd="dnf install -y"
            # Specific check for Oracle Linux 8/9 repo issues
            if [[ "$OS_ID" == "ol" ]]; then
                echo -e "${BLUE}Optimizing for Oracle Linux... Enabling AppStream...${NC}"
                sudo dnf config-manager --set-enabled ol${OS_VERSION%%.*}_appstream >/dev/null 2>&1 || true
            fi
            ;;
        *)
            echo -e "${RED}Unsupported distribution: $OS_ID${NC}"
            echo -e "Required: java-21, sqlite3, curl, build-essential"
            exit 1
            ;;
    esac

    echo -e "${BLUE}Installing dependencies: $@...${NC}"
    if ! sudo $cmd $@; then
        echo -e "${YELLOW}Installation failed. Attempting auto-fix...${NC}"
        if [[ "$OS_ID" == "ubuntu" || "$OS_ID" == "debian" ]]; then
            sudo apt-get install -f -y
            sudo $cmd $@
        elif [[ "$OS_ID" == "ol" || "$OS_ID" == "rhel" ]]; then
            sudo dnf clean all
            sudo $cmd $@
        else
            echo -e "${RED}Could not automatically fix installation. Please check your internet or repositories.${NC}"
            exit 1
        fi
    fi
}

# Onboarding Questions
onboard() {
    echo -e "\n${BLUE}--- ONBOARDING CONFIGURATION ---${NC}"
    
    # Admin Password
    while true; do
        read -rsp "Set Admin Password [default: changeme]: " ADMIN_PASS
        echo
        ADMIN_PASS=${ADMIN_PASS:-changeme}
        if [ ${#ADMIN_PASS} -ge 4 ]; then break; fi
        echo -e "${RED}Password too short (min 4 chars).${NC}"
    done

    # Servers Directory
    read -p "Servers Directory [default: ./servers]: " SERVERS_DIR
    SERVERS_DIR=${SERVERS_DIR:-./servers}
    mkdir -p "$SERVERS_DIR"

    # API Port
    read -p "API Port [default: 8080]: " API_PORT
    API_PORT=${API_PORT:-8080}

    # System Tweaks
    read -p "Apply Linux performance tweaks (Limits & THP)? [Y/n]: " DO_TWEAKS
    DO_TWEAKS=${DO_TWEAKS:-Y}

    # Systemd Service
    read -p "Install as a background service (Systemd)? [Y/n]: " DO_SERVICE
    DO_SERVICE=${DO_SERVICE:-Y}

    if [[ "$DO_SERVICE" =~ ^[Yy]$ ]]; then
        read -p "Run service as user [default: $USER]: " SERVICE_USER
        SERVICE_USER=${SERVICE_USER:-$USER}
        
        read -p "Enable start on boot? [Y/n]: " SERVICE_BOOT
        SERVICE_BOOT=${SERVICE_BOOT:-Y}
    fi
}

# Main Logic
main() {
    check_root
    detect_os
    onboard

    # Install specific packages based on OS
    case $OS_ID in
        ubuntu|debian|kali|raspbian)
            install_pkg openjdk-21-jre-headless sqlite3 curl build-essential libssl-dev pkg-config
            ;;
        arch|manjaro)
            install_pkg jre21-openjdk-headless sqlite curl base-devel openssl
            ;;
        fedora|ol|rhel|centos|almalinux|rocky)
            install_pkg java-21-openjdk-headless sqlite curl openssl-devel
            [[ "$OS_ID" == "ol" || "$OS_ID" == "rhel" ]] && install_pkg gcc make pkgconfig
            ;;
    esac

    # Create .env
    echo -e "${BLUE}Generating .env configuration...${NC}"
    cat << EOF > .env
ADMIN_PASSWORD=$ADMIN_PASS
DATABASE_URL=sqlite://mineserv.db
API_HOST=0.0.0.0
API_PORT=$API_PORT
SERVERS_DIR=$SERVERS_DIR
JWT_SECRET=$(head /dev/urandom | tr -dc A-Za-z0-9 | head -c 32)
EOF

    # Rust Toolchain Check/Install
    if ! command -v cargo &> /dev/null; then
        echo -e "\n${YELLOW}Rust toolchain (Cargo) not found.${NC}"
        read -p "Would you like to install the Rust toolchain now? [Y/n]: " INSTALL_RUST
        INSTALL_RUST=${INSTALL_RUST:-Y}
        if [[ "$INSTALL_RUST" =~ ^[Yy]$ ]]; then
            echo -e "${BLUE}Installing Rust via rustup...${NC}"
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
            source "$HOME/.cargo/env"
            echo -e "${GREEN}Rust installed successfully.${NC}"
        else
            echo -e "${YELLOW}Skipping Rust installation. Build will be skipped.${NC}"
        fi
    fi

    # Build Project
    if command -v cargo &> /dev/null; then
        echo -e "${BLUE}Building project with Cargo...${NC}"
        cargo build --release
    else
        echo -e "${YELLOW}Cargo not found. Skipping build. You must build manually later.${NC}"
    fi

    # Apply Tweaks
    if [[ "$DO_TWEAKS" =~ ^[Yy]$ ]]; then
        if [ "$IS_ROOT" = true ]; then
            echo -e "${BLUE}Applying system optimizations...${NC}"
            # File limits
            if ! grep -q "mineserv" /etc/security/limits.conf; then
                cat << EOF >> /etc/security/limits.conf
* soft nofile 100000
* hard nofile 100000
EOF
            fi
            # THP
            if [ -f /sys/kernel/mm/transparent_hugepage/enabled ]; then
                echo madvise > /sys/kernel/mm/transparent_hugepage/enabled
            fi
            echo -e "${GREEN}Performance tweaks applied.${NC}"
        else
            echo -e "${YELLOW}Skipping tweaks: Root access required.${NC}"
        fi
    fi

    # Install Service
    if [[ "$DO_SERVICE" =~ ^[Yy]$ ]]; then
        if [ "$IS_ROOT" = true ]; then
            echo -e "${BLUE}Installing Systemd service...${NC}"
            WORKDIR=$(pwd)
            cat << EOF > /etc/systemd/system/mineserv.service
[Unit]
Description=Mineserv Minecraft Server Manager
After=network.target

[Service]
Type=simple
User=$SERVICE_USER
WorkingDirectory=$WORKDIR
ExecStart=$WORKDIR/target/release/api-server
Restart=always
RestartSec=10
LimitNOFILE=100000
ReadWritePaths=$WORKDIR

[Install]
WantedBy=multi-user.target
EOF
            systemctl daemon-reload
            if [[ "$SERVICE_BOOT" =~ ^[Yy]$ ]]; then
                systemctl enable mineserv
                echo -e "${GREEN}Service enabled on boot.${NC}"
            fi
            echo -e "${GREEN}Systemd service installed successfully.${NC}"
        else
            echo -e "${YELLOW}Skipping service: Root access required.${NC}"
        fi
    fi

    echo -e "\n${GREEN}======================================================"
    echo -e "    Mineserv Intelligent Onboarding Finished!"
    echo -e "======================================================${NC}"
    echo -e "Access Mineserv at: http://localhost:$API_PORT"
    echo -e "Log in with your administrator password to start managing servers."
    exit 0
}

main
