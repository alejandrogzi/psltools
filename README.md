<p align="center">
  <p align="center">
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 160" width="200" height="200">
  <!-- Cross connector -->
  <rect x="65" y="52" width="30" height="16" rx="3" fill="#A86599"/>
  <rect x="65" y="92" width="30" height="16" rx="3" fill="#6E65A8"/>

  <!-- Left link -->
  <rect x="30" y="40" width="40" height="80" rx="6" fill="none" stroke="#A86599" stroke-width="12"/>
  <rect x="30" y="30" width="40" height="20" rx="4" fill="#A86599"/>

  <!-- Right link -->
  <rect x="90" y="40" width="40" height="80" rx="6" fill="none" stroke="#6E65A8" stroke-width="12"/>
  <rect x="90" y="110" width="40" height="20" rx="4" fill="#6E65A8"/>
  </svg>
  </p>

  <span>
    <h1 align="center">
        psltools
    </h1>
  </span>

  <p align="center">
    <a href="https://img.shields.io/badge/version-0.0.1-green" target="_blank">
      <img alt="Version Badge" src="https://img.shields.io/badge/version-0.0.1-green">
    </a>
    <a href="https://crates.io/crates/psltools" target="_blank">
      <img alt="Crates.io Version" src="https://img.shields.io/crates/v/psltools">
    </a>
    <a href="https://github.com/alejandrogzi/psltools" target="_blank">
      <img alt="GitHub License" src="https://img.shields.io/github/license/alejandrogzi/psltools?color=blue">
    </a>
    <a href="https://crates.io/crates/psltools" target="_blank">
      <img alt="Crates.io Total Downloads" src="https://img.shields.io/crates/d/psltools">
    </a>
  </p>

  <p align="center">
    <samp>
        <span>work with .chain files in Rust</span>
        <br>
        <br>
        <a href="https://docs.rs/psltools/0.0.1/psltools/">docs</a> .
        <a href="https://github.com/alejandrogzi/psltools/tree/master/assets/usage/usage.md">usage</a> .
        <a href="https://github.com/alejandrogzi/psltools/tree/master/assets/tools">tools</a> .
        <a href="https://www.ensembl.org/info/website/upload/psl.html">psl</a> 
    </samp>
  </p>

</p>


## Installation
### Binary
```bash
cargo install --all-features psltools
```

### Docker
```bash
docker pull ghcr.io/alejandrogzi/psltools:latest
```

### Conda
```bash
conda install -c bioconda psltools
```

### Library
Add this to your `Cargo.toml`:

```toml
[dependencies]
psltools = { version = "0.0.1", features = ["mmap", "gzip", "parallel"] }
```
