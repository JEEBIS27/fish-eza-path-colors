# Optional fish compatibility hook

Stock fish has no API for adding these per-path styles. The provider stays an
independent plugin; these files only connect versioned providers to the native
highlighter. They are a local extension, not an upstream-supported plugin API.

The current baseline is fish 4.9.3. `nix build .#fish` applies the generic patch
to the pinned upstream package. Then use that fish together with the provider
and the `conf.d` file. For example, temporarily start a test shell:

```sh
nix build . --out-link result-plugin
nix build .#fish --out-link result-fish
./result-fish/bin/fish --no-config -C 'source ./result-plugin/conf.d/fish-eza-path-colors.fish'
```

The hook has no dependency on eza and can host other providers using protocol v1.
Port only these compatibility files when upstream changes an internal hook point.
Run both tests in `tests/` with their explicit binary/plugin arguments after a
port. They require Python with `pyte` and `pexpect`.

Selecting stock fish disables the additional colors and preserves normal input.
Never force the capability variable on an unpatched fish.
