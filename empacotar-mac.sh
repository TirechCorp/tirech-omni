#!/bin/sh
# Compila e empacota o Tirech Omni para o macOS, num Mac (Apple Silicon ou Intel), com as
# ferramentas de linha de comando da Apple (xcode-select --install) e o rustup. Precisa do
# ../tomni-ui ao lado, como o build normal.
#
# Uso: ./empacotar-mac.sh [saída.dmg]   (padrão: target/tirech-omni-macos.dmg)
# Compila os dois alvos (aarch64-apple-darwin e x86_64-apple-darwin) e junta cada executável
# num universal com o lipo: o mesmo .dmg abre no Apple Silicon e no Intel. O .dmg traz o
# "Tirech Omni.app", com os cinco executáveis em Contents/MacOS (o app e as ferramentas, que a
# instalação copia juntos), um atalho para /Applications e, se existir, o
# target/THIRD-PARTY-NOTICES.txt do `cargo about` (ou o de $AVISOS).
#
# Assinatura: sem $IDENTIDADE, é a ad-hoc (`codesign -s -`), que o Apple Silicon exige para
# rodar, mas que o Gatekeeper não reconhece: na primeira vez, quem baixou abre em Ajustes do
# Sistema > Privacidade e Segurança > "Abrir Mesmo Assim". Com $IDENTIDADE (o nome de um
# certificado "Developer ID Application" no chaveiro), assina com o runtime endurecido; com
# $PERFIL_NOTARIZACAO também (o perfil gravado por `xcrun notarytool store-credentials`), envia
# o .dmg para a Apple notarizar e grampeia o resultado, e o app abre sem aviso.
set -eu

raiz=$(cd "$(dirname "$0")" && pwd)
mae=$(dirname "$raiz")
saida=${1:-$raiz/target/tirech-omni-macos.dmg}
case $saida in /*) ;; *) saida=$(pwd)/$saida ;; esac
avisos=${AVISOS:-$raiz/target/THIRD-PARTY-NOTICES.txt}
alvo=${CARGO_TARGET_DIR:-$raiz/target}
versao=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$raiz/Cargo.toml" | head -n 1)
nomes="tirech-omni tirech-omni-add tirech-omni-agent tirech-omni-copy-id tirech-omni-keygen"
triplos="aarch64-apple-darwin x86_64-apple-darwin"

if [ "$(uname)" != Darwin ]; then
    echo "O empacotar-mac.sh roda num Mac (o lipo, o codesign e o hdiutil são da Apple)." >&2
    exit 1
fi
if [ ! -d "$mae/tomni-ui" ]; then
    echo "Falta $mae/tomni-ui: clone a lib ao lado deste repositório." >&2
    exit 1
fi

# O rust-toolchain.toml escolhe a versão; os dois alvos entram nela.
(cd "$raiz" && rustup target add $triplos)
for triplo in $triplos; do
    echo "== $triplo"
    (cd "$raiz" && cargo build --release --locked --target "$triplo" \
        -p tirech-omni -p tirech-omni-tools)
done

trabalho=$(mktemp -d)
trap 'rm -rf "$trabalho"' EXIT
app="$trabalho/Tirech Omni/Tirech Omni.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

for nome in $nomes; do
    lipo -create -output "$app/Contents/MacOS/$nome" \
        "$alvo/aarch64-apple-darwin/release/$nome" "$alvo/x86_64-apple-darwin/release/$nome"
    strip -x "$app/Contents/MacOS/$nome"
done
cp "$raiz/crates/tirech-omni/assets/icone.icns" "$app/Contents/Resources/tirech-omni.icns"
# O mesmo identificador do LaunchAgent do início com o sistema (sistema.rs).
cat > "$app/Contents/Info.plist" <<FIM
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>pt-BR</string>
	<key>CFBundleDisplayName</key>
	<string>Tirech Omni</string>
	<key>CFBundleExecutable</key>
	<string>tirech-omni</string>
	<key>CFBundleIconFile</key>
	<string>tirech-omni</string>
	<key>CFBundleIdentifier</key>
	<string>com.tirech.omni</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>Tirech Omni</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$versao</string>
	<key>CFBundleVersion</key>
	<string>$versao</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSLocalNetworkUsageDescription</key>
	<string>O Tirech Omni conecta por SSH, Telnet, SFTP e FTP aos computadores da sua rede.</string>
</dict>
</plist>
FIM
# O pedido da Rede Local nos outros idiomas (o macOS bloqueia a rede local sem ele).
mkdir -p "$app/Contents/Resources/en.lproj"
printf '"NSLocalNetworkUsageDescription" = "Tirech Omni connects over SSH, Telnet, SFTP and FTP to the computers on your network.";\n' \
    > "$app/Contents/Resources/en.lproj/InfoPlist.strings"
mkdir -p "$app/Contents/Resources/es.lproj"
printf '"NSLocalNetworkUsageDescription" = "Tirech Omni se conecta por SSH, Telnet, SFTP y FTP a los equipos de tu red.";\n' \
    > "$app/Contents/Resources/es.lproj/InfoPlist.strings"
plutil -lint "$app/Contents/Info.plist" >/dev/null

# Primeiro as ferramentas, depois o .app, que assina o tirech-omni (o CFBundleExecutable) e sela
# o Info.plist e o ícone: o codesign recusa o .app com um executável de dentro sem assinatura.
if [ -n "${IDENTIDADE:-}" ]; then
    assinar() { codesign --force --options runtime --timestamp -s "$IDENTIDADE" "$1"; }
else
    assinar() { codesign --force -s - "$1"; }
fi
for nome in $nomes; do
    if [ "$nome" != tirech-omni ]; then
        assinar "$app/Contents/MacOS/$nome"
    fi
done
assinar "$app"
codesign --verify --strict "$app"

ln -s /Applications "$trabalho/Tirech Omni/Applications"
if [ -f "$avisos" ]; then
    cp "$avisos" "$trabalho/Tirech Omni/THIRD-PARTY-NOTICES.txt"
fi

mkdir -p "$(dirname "$saida")"
rm -f "$saida"
hdiutil create -quiet -volname "Tirech Omni" -srcfolder "$trabalho/Tirech Omni" \
    -fs HFS+ -format UDZO "$saida"
if [ -n "${IDENTIDADE:-}" ]; then
    codesign --force --timestamp -s "$IDENTIDADE" "$saida"
    if [ -n "${PERFIL_NOTARIZACAO:-}" ]; then
        xcrun notarytool submit "$saida" --keychain-profile "$PERFIL_NOTARIZACAO" --wait
        xcrun stapler staple "$saida"
    fi
fi
echo "$saida"
