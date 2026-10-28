#!/bin/zsh
# zTerm for other Macs: target/zterm/zTerm-<version>.dmg, the app (built and
# signed by scripts/zterm-app.sh) beside a link to /Applications, signed with
# a Developer ID, notarized and stapled, so Gatekeeper opens it offline.
# ./scripts/zterm-release.sh. ZTERM_SIGN_IDENTITY picks the Developer ID
# Application identity (else the first); NOTARY_PROFILE the notarytool
# keychain profile (`xcrun notarytool store-credentials`, default swgl-notary).
set -eu
root=${0:A:h:h}
cd $root
identity=${ZTERM_SIGN_IDENTITY:-$(security find-identity -v -p codesigning |
  grep '"Developer ID Application' | grep -v CSSMERR | head -1 | awk '{print $2}')}
[[ -n $identity ]] || { echo "no Developer ID Application identity to sign with" >&2; exit 1 }
profile=${NOTARY_PROFILE:-swgl-notary}
version=$(sed -n 's|.*<key>CFBundleShortVersionString</key><string>\([^<]*\)</string>.*|\1|p' scripts/zterm-app.sh)

stage=$(mktemp -d)
trap 'rm -rf $stage' EXIT
mkdir $stage/dmg
ZTERM_REGISTER=0 ZTERM_SIGN_IDENTITY=$identity ./scripts/zterm-app.sh $stage/dmg
codesign --verify --strict --deep $stage/dmg/zTerm.app
ln -s /Applications $stage/dmg/Applications

mkdir -p target/zterm
dmg=target/zterm/zTerm-$version.dmg
rm -f $dmg
hdiutil create -quiet -volname "zTerm $version" -srcfolder $stage/dmg -fs HFS+ -format ULMO $dmg
codesign --force --timestamp --sign $identity $dmg
xcrun notarytool submit $dmg --keychain-profile $profile --wait
xcrun stapler staple $dmg
xcrun stapler validate $dmg
spctl --assess --type open --context context:primary-signature --verbose $dmg
echo "released $root/$dmg"
