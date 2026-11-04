# Installation Guide

`ripwire-rs` ships prebuilt binaries and native system packages for macOS (`aarch64`, `x86_64`), Linux (`x86_64`, `aarch64`), and Windows (`x86_64`), as well as source builds via Cargo. All installation methods provide both the `ripwire-rs` command and the `rw` shorthand executable.

---

## Homebrew (macOS & Linux)

```bash
brew install casonadams/tap/ripwire-rs
```

Upgrade to the latest release:

```bash
brew update && brew upgrade ripwire-rs
```

---

## APT Repository (Ubuntu & Debian — `amd64` & `arm64`)

Add the APT repository hosted on GitHub Pages and install `ripwire-rs`:

```bash
echo "deb [trusted=yes] https://casonadams.github.io/homebrew-tap/apt stable main" | sudo tee /etc/apt/sources.list.d/casonadams.list
sudo apt update && sudo apt install ripwire-rs
```

Upgrade to the latest release:

```bash
sudo apt update && sudo apt upgrade ripwire-rs
```

Or download a standalone `.deb` asset directly from [GitHub Releases](https://github.com/casonadams/ripwire-rs/releases/latest):

```bash
gh release download --repo casonadams/ripwire-rs --pattern "ripwire-rs_*_amd64.deb"
sudo dpkg -i ripwire-rs_*_amd64.deb
```

---

## DNF / RPM Repository (Fedora, RHEL & CentOS — `x86_64` & `aarch64`)

Add the RPM repository and install `ripwire-rs`:

```bash
curl -fsSL https://casonadams.github.io/homebrew-tap/rpm/casonadams.repo | sudo tee /etc/yum.repos.d/casonadams.repo
sudo dnf install ripwire-rs
```

Upgrade to the latest release:

```bash
sudo dnf upgrade --refresh ripwire-rs
```

---

## Cargo & `cargo-binstall`

Install the prebuilt binary in ~1 second using [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall):

```bash
cargo binstall ripwire-rs
```

Or compile from crates.io source:

```bash
cargo install ripwire-rs
```

---

## From Source

```bash
git clone https://github.com/casonadams/ripwire-rs.git
cd ripwire-rs
make install
```

Builds the optimized release binaries (`ripwire-rs` and `rw`) and installs the manual pages (`man ripwire-rs` and `man rw`).

---

## Post-Install: Agent Skills (Recommended) vs. MCP Server

If your coding agent supports skills and shell execution, install the 9 embedded skills—**no MCP server configuration is required**, as the skills guide the agent to run `ripwire-rs` CLI flags directly:
```bash
# Install 9 ripwire-rs agent skills
ripwire-rs --add-skills

# Verify installation and environment health
ripwire-rs . --doctor
```

If your client only supports Model Context Protocol tools (or lacks shell/skill support), see **[MCP Configuration](MCP.md)** to register `ripwire-rs mcp`.

---

## Security, Privacy & Zero Telemetry

- **Air-Gapped & Offline**: `ripwire-rs` operates 100% locally with zero internet access, zero telemetry, and zero phone-home beacons.
- **Zero Training**: Code is never transmitted to external servers or used to train models.
- **Local Context Only**: The binary parses local Tree-Sitter ASTs and outputs structured XML/JSON to stdout or stdio MCP for consumption by your configured agent harness.
- **Automatic Secret Masking**: Detects and redacts secrets and authentication credentials from emitted source snippets by default.
