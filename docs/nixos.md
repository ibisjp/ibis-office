# ibisOffice on NixOS

ibisOffice has no official package in nixpkgs yet. A package is being prepared (see [Status](#status)). Until then, you can run the official release files on NixOS in either of these ways. The Flatpak is the recommended one.

The files are on the [Releases](https://github.com/ibisjp/ibis-office/releases) page. The examples below use version 0.8.9; replace it with the version you want.

| File | Architecture |
| --- | --- |
| `ibisOffice-0.8.9-Linux-x86_64.flatpak` | x86_64 |
| `ibisOffice-0.8.9-Linux-aarch64.flatpak` | aarch64 |
| `ibisOffice-0.8.9-Linux.AppImage` | x86_64 |

Each file has a `.sha256` file beside it.

## Option 1: the Flatpak (recommended)

The Flatpak application ID is `jp.ne.ibis.ibisOffice`. It uses the `org.freedesktop.Platform` runtime, version 24.08, from Flathub.

### Enable Flatpak

In `configuration.nix`:

```nix
services.flatpak.enable = true;
```

Then add the Flathub remote once (this is the command shown on the [NixOS Wiki](https://wiki.nixos.org/wiki/Flatpak); the runtime comes from Flathub):

```sh
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
```

### Install by hand

```sh
flatpak install --system ./ibisOffice-0.8.9-Linux-x86_64.flatpak
flatpak run jp.ne.ibis.ibisOffice
```

Use `--user` instead of `--system` to install for the current user only. The wiki notes that a per-user install may need `XDG_DATA_DIRS` to include `$HOME/.local/share/flatpak/exports/share` so that the launcher entry appears.

### Install in your configuration with nix-flatpak

[nix-flatpak](https://github.com/gmodena/nix-flatpak) installs Flatpak applications from your NixOS configuration. Add it as a flake input and add its module:

```nix
# flake.nix
inputs.nix-flatpak.url = "github:gmodena/nix-flatpak";

# in the system's modules list
modules = [
  nix-flatpak.nixosModules.nix-flatpak
  ./configuration.nix
];
```

Then install the release bundle. `nix-flatpak` accepts a `bundle` together with the `appId` and a `sha256`:

```nix
{ pkgs, ... }:
{
  services.flatpak.enable = true;

  services.flatpak.packages = [
    rec {
      appId = "jp.ne.ibis.ibisOffice";
      sha256 = "<hash>";
      bundle = "${pkgs.fetchurl {
        url = "https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Linux-x86_64.flatpak";
        inherit sha256;
      }}";
    }
  ];
}
```

For `<hash>`, use the SHA-256 of the file. The `.sha256` file in the release holds it in hexadecimal; you can convert it with `nix hash convert --hash-algo sha256 --to sri <hex>`, or leave the value empty once and copy the hash that the failed build reports.

Two things to know about `nix-flatpak` (from its README):

- If you set `services.flatpak.remotes`, it replaces the default `flathub` remote. Declare `flathub` yourself, or use `lib.mkOptionDefault` to add to the default.
- Pin the module to a release, for example `github:gmodena/nix-flatpak/?ref=v0.8.0`, if you do not want it to change under you.

A user who installed ibisOffice this way on NixOS 26.05 with nix-flatpak 0.8.0 reported that it installs, starts after a reboot, and opens a CSV and a DOCX file ([issue #269](https://github.com/ibisjp/ibis-office/issues/269)).

## Option 2: the AppImage

AppImages cannot be run directly on NixOS, because they expect libraries at fixed paths. Two ways to run the official AppImage:

### With appimage-run

```sh
nix-shell -p appimage-run
appimage-run ./ibisOffice-0.8.9-Linux.AppImage
```

### Let NixOS run AppImages directly

From the NixOS Wiki, these two options register `appimage-run` as the program that opens AppImage files:

```nix
programs.appimage.enable = true;
programs.appimage.binfmt = true;
```

After a rebuild, `chmod +x` the file and run it as usual.

### Wrap it as a Nix package

`appimageTools.wrapType2` unpacks an AppImage and runs it with its dependencies as Nix inputs:

```nix
{ appimageTools, fetchurl }:

appimageTools.wrapType2 {
  pname = "ibisoffice";
  version = "0.8.9";
  src = fetchurl {
    url = "https://github.com/ibisjp/ibis-office/releases/download/v0.8.9/ibisOffice-0.8.9-Linux.AppImage";
    hash = "<hash>";
  };
}
```

The same user in [issue #269](https://github.com/ibisjp/ibis-office/issues/269) reported that this builds and opens documents. We have not tried the `appimage-run` and `binfmt` routes on NixOS ourselves.

## Status

- The official Linux files above are tested by us on other distributions; on NixOS they have been reported to work by a user (issue #269).
- The nixpkgs package (`ibisoffice`) is not merged yet. A pull request from the community is open: [NixOS/nixpkgs#591630](https://github.com/NixOS/nixpkgs/pull/591630). When a package is merged, this page will say so and show the install line.
- Found a problem on NixOS? Please [open an issue](https://github.com/ibisjp/ibis-office/issues) and include your NixOS version and how you installed ibisOffice.

## Sources

- [NixOS Wiki: Flatpak](https://wiki.nixos.org/wiki/Flatpak)
- [NixOS Wiki: Appimage](https://wiki.nixos.org/wiki/Appimage)
- [nix-flatpak README](https://github.com/gmodena/nix-flatpak)
