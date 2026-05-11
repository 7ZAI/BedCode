#!/bin/bash
# BedCode WebSocket Connection Library

# Get BedCode port from platform-specific location
get_bedcode_port() {
    local port_file=""

    case "$(uname -s)" in
        CYGWIN*|MINGW*|MSYS*)
            port_file="$APPDATA/com.bedcode.app/bedcode-port.txt"
            ;;
        Darwin*)
            port_file="$HOME/Library/Application Support/com.bedcode.app/bedcode-port.txt"
            ;;
        *)
            port_file="$HOME/.config/com.bedcode.app/bedcode-port.txt"
            ;;
    esac

    if [ -f "$port_file" ]; then
        cat "$port_file"
    else
        # Default port
        echo "9527"
    fi
}

# Send WebSocket message using socat or netcat
# Args: $1 = port, $2 = JSON message
send_ws_message() {
    local port="$1"
    local message="$2"

    # Try socat first (more reliable for WS)
    if command -v socat &> /dev/null; then
        echo "$message" | socat - TCP:127.0.0.1:$port
        return $?
    fi

    # Fallback to netcat (nc)
    if command -v nc &> /dev/null; then
        echo "$message" | nc -q 1 127.0.0.1 $port
        return $?
    fi

    # Fallback to curl with websocket
    if command -v curl &> /dev/null; then
        curl -s -w "%{http_code}" -X POST \
            -H "Content-Type: application/json" \
            -d "$message" \
            "http://127.0.0.1:$port/ws" 2>/dev/null
        return $?
    fi

    echo '{"error": "no websocket client available (install socat or nc)"}'
    return 1
}

# Check if BedCode is running
check_bedcode_running() {
    local port=$(get_bedcode_port)

    # Try to connect to the port
    if command -v nc &> /dev/null; then
        if nc -z 127.0.0.1 $port 2>/dev/null; then
            return 0
        fi
    elif command -v curl &> /dev/null; then
        if curl -s "http://127.0.0.1:$port" > /dev/null 2>&1; then
            return 0
        fi
    fi

    return 1
}