# File Searcher (fsearch)

A high-performance command-line file searching tool built with Rust.

## Features

- Fast file content searching using memory mapping
- Parallel processing with Rayon
- Regular expression support
- Colored output for better readability
- Progress indication during search
- Memory-efficient handling of large files

## Installation

### Prerequisites

- Rust and Cargo (latest stable version)

### Building from Source

```bash
git clone https://github.com/yourusername/file_searcher.git
cd file_searcher
cargo build --release
