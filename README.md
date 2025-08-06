# Crabidy 🦀🎵

A high-performance, terminal-based music streaming client written in Rust that provides seamless access various music sources through an intuitive vim-like interface.

## Features

- **Terminal User Interface**: Clean, responsive TUI with vim-style navigation
- **TIDAL Integration**: Full access to TIDAL's music library including playlists, artists, and albums
- **Real-time Playback Control**: Play, pause, skip, volume control, and queue management
- **Library Browsing**: Navigate through your playlists, favorite artists, and albums
- **Queue Management**: Add, remove, reorder tracks with intuitive keyboard shortcuts
- **Audio Streaming**: High-quality audio playback using Rodio and Symphonia
- **Cross-platform**: Supports Linux ARM, ARM64, and x86_64 architectures
- **Modular Architecture**: Plugin-based provider system for future streaming service support

## Architecture

Crabidy follows a client-server architecture with the following components:

```
┌─────────────────┐    gRPC     ┌──────────────────┐
│    cbd-tui      │◄───────────►│ crabidy-server   │
│   (Terminal     │             │   (Playback      │
│    Client)      │             │   & Queue Mgmt)  │
└─────────────────┘             └──────────────────┘
                                          │
                                          ▼
                                ┌──────────────────┐
                                │   tidaldy        │
                                │  (TIDAL API)     │
                                └──────────────────┘
                                          │
                                          ▼
                                ┌──────────────────┐
                                │  audio-player    │
                                │  (Rodio/Symphonia) │
                                └──────────────────┘
```

## Prerequisites

- **Rust**: 1.70 or later
- **TIDAL Subscription**: Required for music streaming access

## Installation

### From Source

1. **Clone the repository**:
   ```bash
   git clone https://github.com/your-username/crabidy.git
   cd crabidy
   ```

2. **Build the project**:
   ```bash
   cargo build --release
   ```

3. **Install binaries** (optional):
   ```bash
   cargo install --path crabidy-server
   cargo install --path cbd-tui
   ```

### Cross-compilation

For different architectures:

```bash
# Install cross compilation tool
cargo install cross

# Build for ARM64
cross build --target aarch64-unknown-linux-gnu --release

# Build for ARM v7
cross build --target armv7-unknown-linux-gnueabihf --release

# Build for x86_64
cross build --target x86_64-unknown-linux-gnu --release
```

## Quick Start

1. **Start the server**:
   ```bash
   cargo run --bin crabidy-server
   ```
   
   On first run, the server will create a configuration file at `~/.config/crabidy/tidaldy.toml`.

2. **Configure TIDAL authentication**:
   The server will prompt you to visit a TIDAL authentication URL on first startup.

3. **Launch the TUI client** (in a separate terminal):
   ```bash
   cargo run --bin cbd-tui
   ```

## Usage

### Navigation

Crabidy uses vim-inspired keyboard shortcuts:

#### Global Controls
- `q` - Quit application
- `Tab` - Cycle between panels (Library/Queue)
- `Space` - Toggle play/pause
- `r` - Restart current track
- `Shift+J/K` - Volume down/up
- `m` - Toggle mute
- `z` - Toggle shuffle
- `x` - Toggle repeat

#### Library Navigation
- `j/k` - Move down/up
- `h/l` - Go back/enter directory or item
- `g/G` - Go to first/last item
- `Ctrl+d/u` - Page down/up
- `Enter` - Replace queue with selection
- `a` - Append to queue
- `Shift+L` - Queue selection
- `s` - Mark/unmark for multi-select

#### Queue Management
- `j/k` - Move down/up in queue
- `g/G` - Go to first/last track
- `Ctrl+d/u` - Page down/up
- `Enter` - Play selected track
- `o` - Jump to currently playing track
- `d` - Remove track from queue
- `c` - Clear queue (keep current track)
- `Shift+C` - Clear entire queue
- `p` - Insert track at current position

#### Playback Controls
- `Ctrl+n/p` - Next/previous track

### Configuration

Configuration files are stored in `~/.config/crabidy/`:

- `tidaldy.toml` - TIDAL provider configuration
- `cbd-tui.toml` - TUI client settings

Example TIDAL configuration:
```toml
[login]
access_token = "your_access_token"
refresh_token = "your_refresh_token"
user_id = "your_user_id"
country_code = "US"

[oauth]
client_id = "tidal_client_id"
client_secret = "tidal_client_secret"

hifi_url = "https://api.tidalhifi.com/v1"
base_url = "https://api.tidal.com/v1"
```

## Development

### Project Structure

```
crabidy/
├── audio-player/          # GStreamer audio playback engine
├── cbd-tui/              # Terminal user interface
├── crabidy-core/         # Core traits and protocol definitions
├── crabidy-server/       # gRPC server and orchestration
├── stream-download/      # Audio streaming utilities
├── tidaldy/             # TIDAL API client and provider
├── Cross.toml           # Cross-compilation configuration
└── Cargo.toml           # Workspace definition
```

### Building and Testing

```bash
# Build entire workspace
cargo build

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run --bin crabidy-server

# Format code
cargo fmt --all

# Run clippy
cargo clippy --all-targets --all-features
```

### Adding New Music Providers

1. Create a new crate implementing the `ProviderClient` trait from `crabidy-core`
2. Implement required methods:
   - `init()` - Initialize with configuration
   - `get_lib_root()` - Return root library node
   - `get_lib_node()` - Fetch library content
   - `get_urls_for_track()` - Get streaming URLs
   - `get_metadata_for_track()` - Get track metadata

3. Register the provider in `crabidy-server/src/provider.rs`

Example:
```rust
use async_trait::async_trait;
use crabidy_core::{ProviderClient, ProviderError};

#[derive(Debug)]
pub struct MyMusicProvider {
    // Your provider fields
}

#[async_trait]
impl ProviderClient for MyMusicProvider {
    async fn init(config: &str) -> Result<Self, ProviderError> {
        // Initialize your provider
    }
    
    // Implement other required methods...
}
```

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes and add tests
4. Run the test suite: `cargo test`
5. Format your code: `cargo fmt --all`
6. Run clippy: `cargo clippy --all-targets --all-features`
7. Commit your changes: `git commit -m 'Add amazing feature'`
8. Push to the branch: `git push origin feature/amazing-feature`
9. Open a Pull Request

### Code Style

- Follow standard Rust conventions
- Use `cargo fmt` for consistent formatting
- Ensure `cargo clippy` passes without warnings
- Write tests for new functionality
- Document public APIs

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- TIDAL API integration inspired by [tdl](https://github.com/MinisculeGirraffe/tdl)
- Built with [Ratatui](https://github.com/ratatui-org/ratatui) for the terminal interface
- Audio playback powered by [Rodio](https://github.com/RustAudio/rodio) and [Symphonia](https://github.com/pdeljanov/Symphonia)

## Disclaimer

This project is not affiliated with TIDAL. You must have a valid TIDAL subscription to use this software. Users are responsible for complying with TIDAL's Terms of Service.
