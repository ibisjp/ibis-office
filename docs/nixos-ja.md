# NixOS で ibisOffice を使う

ibisOffice の公式パッケージは、nixpkgs にはまだありません。パッケージは準備中です（[状況](#状況)）。それまでは、公式の配布ファイルを、次のどちらかの方法で NixOS で動かせます。おすすめは Flatpak です。

ファイルは [Releases](https://github.com/ibisjp/ibis-office/releases) にあります。例は 0.8.9 で書きます。使いたい版に読み替えてください。

| ファイル | アーキテクチャ |
| --- | --- |
| `ibisOffice-0.8.9-Linux-x86_64.flatpak` | x86_64 |
| `ibisOffice-0.8.9-Linux-aarch64.flatpak` | aarch64 |
| `ibisOffice-0.8.9-Linux.AppImage` | x86_64 |

各ファイルの隣に、`.sha256` のファイルがあります。

## 方法 1：Flatpak（おすすめ）

Flatpak のアプリ ID は `jp.ne.ibis.ibisOffice` です。ランタイムは Flathub の `org.freedesktop.Platform` 24.08 を使います。

### Flatpak を有効にする

`configuration.nix` に書きます。

```nix
services.flatpak.enable = true;
```

次に、Flathub のリモートを 1 回だけ追加します（[NixOS Wiki](https://wiki.nixos.org/wiki/Flatpak) にあるコマンドです。ランタイムは Flathub から取ります）。

```sh
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
```

### 手で入れる

```sh
flatpak install --system ./ibisOffice-0.8.9-Linux-x86_64.flatpak
flatpak run jp.ne.ibis.ibisOffice
```

今のユーザーだけに入れるときは、`--system` の代わりに `--user` を使います。Wiki によると、ユーザーごとの導入では、ランチャーに項目が出るように `XDG_DATA_DIRS` に `$HOME/.local/share/flatpak/exports/share` を含める必要がある場合があります。

### nix-flatpak で設定に書く

[nix-flatpak](https://github.com/gmodena/nix-flatpak) を使うと、Flatpak のアプリを NixOS の設定に書いて入れられます。flake の入力に加え、モジュールを足します。

```nix
# flake.nix
inputs.nix-flatpak.url = "github:gmodena/nix-flatpak";

# システムの modules のリストに
modules = [
  nix-flatpak.nixosModules.nix-flatpak
  ./configuration.nix
];
```

次に、Releases のバンドルを入れます。`nix-flatpak` は、`bundle` に `appId` と `sha256` を添えて受け取ります。

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

`<hash>` には、ファイルの SHA-256 を書きます。Releases の `.sha256` は 16 進数です。`nix hash convert --hash-algo sha256 --to sri <16進数>` で変換するか、いったん空にしてビルドを失敗させ、報告されたハッシュを写してください。

`nix-flatpak` の README にある注意が 2 つあります。

- `services.flatpak.remotes` を書くと、既定の `flathub` のリモートが置き換わります。`flathub` も自分で書くか、`lib.mkOptionDefault` で既定に足してください。
- モジュールが勝手に変わらないように、`github:gmodena/nix-flatpak/?ref=v0.8.0` のように版を固定できます。

NixOS 26.05 と nix-flatpak 0.8.0 でこの方法で入れた方が、インストールでき、再起動後も起動し、CSV と DOCX を開けたと報告してくださっています（[issue #269](https://github.com/ibisjp/ibis-office/issues/269)）。

## 方法 2：AppImage

NixOS では、AppImage はそのままでは動きません。決まった場所にあるはずのライブラリが無いためです。公式の AppImage を動かす方法は 3 つあります。

### appimage-run で動かす

```sh
nix-shell -p appimage-run
appimage-run ./ibisOffice-0.8.9-Linux.AppImage
```

### NixOS に AppImage を直接動かさせる

NixOS Wiki にある次の 2 つの設定で、AppImage を開くプログラムとして `appimage-run` が登録されます。

```nix
programs.appimage.enable = true;
programs.appimage.binfmt = true;
```

再構築した後は、`chmod +x` して、ふつうに実行できます。

### Nix のパッケージとして包む

`appimageTools.wrapType2` は、AppImage を展開し、必要なものを Nix の入力として足して動かします。

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

[issue #269](https://github.com/ibisjp/ibis-office/issues/269) の同じ方が、これでビルドでき、文書を開けたと報告しています。`appimage-run` と `binfmt` の方法は、私たちは NixOS ではまだ試していません。

## 状況

- 上の公式の Linux 向けファイルは、NixOS では、利用者から動いたと報告をもらっています（issue #269）。
- nixpkgs のパッケージ（`ibisoffice`）は、まだ nixpkgs に入っていません。コミュニティからのプル リクエストが出ています：[NixOS/nixpkgs#591630](https://github.com/NixOS/nixpkgs/pull/591630)。パッケージが取り込まれたら、このページに知らせ、導入の 1 行を書きます。
- NixOS で問題があれば、NixOS の版と導入のしかたを添えて [issue](https://github.com/ibisjp/ibis-office/issues) で知らせてください。

## 参考にしたページ

- [NixOS Wiki: Flatpak](https://wiki.nixos.org/wiki/Flatpak)
- [NixOS Wiki: Appimage](https://wiki.nixos.org/wiki/Appimage)
- [nix-flatpak README](https://github.com/gmodena/nix-flatpak)
