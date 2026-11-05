# fish-eza-path-colors

An optional fish plugin that uses eza's own filename styles for existing paths in
interactive input. eza handles file kinds, extensions, `EZA_COLORS`, `LS_COLORS`
and YAML themes. The provider is a standalone Rust binary with no fish or third
party Rust dependencies.

## Compatibility

- Full highlighting needs the optional **path-style provider hook v1** in fish.
  The optional `.#fish` package supplies that generic hook. It is a local extension,
  not an upstream fish plugin API.
- Stock fish still starts normally. The plugin checks
  `$fish_path_style_provider_protocol` and stays inactive if absent or unsupported.
- Missing providers, unsupported protocol replies, errors and timeouts fall back
  to fish's normal highlighting. No raw provider output is sent to the terminal.
- The provider protocol is independent of fish's version, Rust ABI and internals.
  Only the small generic hook has to be checked when rebasing fish.
- Verified baseline: fish 4.9.3, with and without the hook. This is not a promise
  of compatibility with untested future fish releases.

## Install

Clone the public repository:

```sh
git clone https://github.com/JEEBIS27/fish-eza-path-colors.git
cd fish-eza-path-colors
```

With Nix, `nix build .` builds the plugin/provider. `nix build .#fish` builds the
optional hook-enabled fish from the pinned upstream baseline. See
[`compat/README.md`](compat/README.md) for details.

For a standalone Cargo install, build the provider and put it on PATH:

```sh
EZA_EXECUTABLE="$(command -v eza)" cargo install --locked --path .
```

Then install `conf.d/fish-eza-path-colors.fish` with your fish plugin manager,
or copy it into `~/.config/fish/conf.d/`.
The hook is still required for additional colors; installing the plugin alone
cannot extend stock fish's native highlighter.

For Home Manager, use the packaged plugin as both a package and a fish plugin:

```nix
let pathColors = import /path/to/fish-eza-path-colors { inherit pkgs; };
in {
  home.packages = [ pathColors ];
  programs.fish.plugins = [{ name = "fish-eza-path-colors"; src = pathColors; }];
}
```

The fish package must also provide the v1 hook. Stock fish stays usable with
standard highlighting.

Disable with `set -g fish_eza_path_colors_enabled 0`, enable with `1`.
The old `fish_color_path_by_type` switch remains supported. The plugin never
replaces key bindings, `fish_prompt`, or the command-line editor.

## Path-style protocol v1

The hook runs one executable directly, without a shell:

```text
provider --fish-path-style-v1 /absolute/path
```

A successful reply is one ASCII line of at most 128 bytes:

```text
FISH_PATH_STYLE 1 FG BG FLAGS
```

Colors: `0` is the terminal default; `1..256` encode palette entries `0..255`;
`0x1000000 | RRGGBB` encodes RGB (written as a decimal integer).
Flags: bold=1, dim=2, italic=4, underline=8, reverse=16, strike=32.
Unknown versions, other color values, extra fields or flags are rejected.
`provider --protocol-version` prints `1`; it does not invoke eza.

The hook is opt-in through these fish variables:

- `fish_color_path_style_provider`: absolute executable path.
- `fish_color_path_style_provider_enabled`: `1` to enable.
- `fish_color_path_style_provider_environment`: environment variable names to
  forward and include in the cache key. This list belongs to the plugin.

The hook resolves paths without running command substitutions, performs I/O only
in fish's background highlighter, caches at most 256 results for two seconds,
and limits each provider call to 80 ms. A timeout kills the provider's process
group, including its helpers. The eza provider has its own 60 ms inner deadline.
Quotes, variables, operators, errors, selections and history-search highlights
keep fish's normal precedence. Prefix-only paths keep native highlighting.

## Updating fish or eza

1. Build the pinned fish with the hook and run `tests/check-fish-path-colors.py` with
   `--plugin /path/to/conf.d/fish-eza-path-colors.fish`.
2. Check stock-fish fallback and malformed/unsupported provider responses with
   `tests/check-fish-provider-compat.py`.
3. If upstream changed a hook location, port only `path-style-provider.patch`
   and its adapter. The eza provider does not need a Rust ABI rebuild for fish.
4. To use stock fish while porting, select upstream `pkgs.fish` in Home Manager.
   The Dotfiles integration also offers `jeebis.shell.enablePathStyleProvider = false`.

The `compat/` files are an optional generic fish hook, separate from the eza
provider. Updating eza or the provider does not require rebuilding fish.
