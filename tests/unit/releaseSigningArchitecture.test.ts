import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

function source(path: string): string {
  return readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8')
}

describe('macOS release signing architecture', () => {
  it('notarizes and staples the final app before packaging distributable artifacts', () => {
    const build = source('../../src-widget/build.sh')
    const finalAppSign = build.indexOf('--entitlements "$MAIN_ENTITLEMENTS" "$APP_BUNDLE"')
    const appNotarization = build.indexOf('submit_for_notarization "$NOTARY_ZIP"')
    const dmgCreation = build.indexOf('echo "=> Creating DMG..."')
    const dmgNotarization = build.indexOf('submit_for_notarization "$DMG_PATH"')
    const updaterCreation = build.indexOf('echo "=> Creating updater artifacts..."')

    expect(finalAppSign).toBeGreaterThan(-1)
    expect(appNotarization).toBeGreaterThan(finalAppSign)
    expect(build.indexOf('stapler staple "$APP_BUNDLE"')).toBeGreaterThan(appNotarization)
    expect(dmgCreation).toBeGreaterThan(appNotarization)
    expect(dmgNotarization).toBeGreaterThan(dmgCreation)
    expect(build.indexOf('stapler staple "$DMG_PATH"')).toBeGreaterThan(dmgNotarization)
    expect(updaterCreation).toBeGreaterThan(dmgNotarization)
    expect(build).toContain('EXPECTED_AUTHORITY="$SIGN_ID"')
    expect(build).toContain('^[A-Fa-f0-9]{40}$')
    expect(build).toContain('CODESIGN_ARGS+=(--timestamp)')
    expect(build).toContain('IDENTITY_LINE=$(grep -F "$SIGN_ID"')
    expect(build).toContain('for BIN in "$APP_BUNDLE/Contents/MacOS/"*')
    expect(build).toContain('"$PLUGINS_DIR/MonetWidgetExtension.appex"')
    expect(build).toContain('"$APP_BUNDLE/Contents/Library/LoginItems/MonetTray.app"')

    const afterAppStaple = build.slice(build.indexOf('stapler staple "$APP_BUNDLE"'))
    const laterAppMutation = afterAppStaple
      .split('\n')
      .some(line => /^\s*codesign\b/.test(line))
    expect(laterAppMutation).toBe(false)
    expect(build.match(/submit_for_notarization /g)).toHaveLength(2)
    expect(build).toContain('--wait || submit_status=$?')
  })

  it('keeps release secrets scoped to their exact signing operations', () => {
    const build = source('../../src-widget/build.sh')
    const secretCapture = build.indexOf('NOTARY_PRIVATE_KEY="${APPLE_API_PRIVATE_KEY:-}"')
    const secretScrub = build.indexOf('unset APPLE_API_PRIVATE_KEY APPLE_API_KEY_PATH')
    const firstBuildTool = build.indexOf('xcodegen generate')
    const keyMaterialization = build.indexOf('printf \'%s\' "$NOTARY_PRIVATE_KEY"')
    const signerInjection = build.indexOf('TAURI_SIGNING_PRIVATE_KEY="$UPDATER_SIGNING_PRIVATE_KEY"')
    const releaseFn = build.slice(build.indexOf('release_notary_key() {'))
    const releaseBody = releaseFn.slice(0, releaseFn.indexOf('\n}'))
    const cleanupFn = build.slice(build.indexOf('cleanup_sensitive_files() {'))
    const cleanupBody = cleanupFn.slice(0, cleanupFn.indexOf('\n}'))

    expect(secretCapture).toBeGreaterThan(-1)
    expect(secretScrub).toBeGreaterThan(secretCapture)
    expect(firstBuildTool).toBeGreaterThan(secretScrub)
    expect(keyMaterialization).toBeGreaterThan(secretScrub)
    // 私钥只有一个落地点，由正式公证与预检共用
    expect(build.match(/printf '%s' "\$NOTARY_PRIVATE_KEY"/g)).toHaveLength(1)
    expect(keyMaterialization).toBeGreaterThan(build.indexOf('materialize_notary_key() {'))
    expect(releaseBody).toContain('rm -f "$NOTARY_KEY_FILE"')
    expect(cleanupBody).toContain('release_notary_key')
    expect(build.indexOf('trap cleanup_sensitive_files EXIT')).toBeLessThan(keyMaterialization)

    // 每次 notarytool 调用都夹在密钥落地与立即清理之间
    const invocations = [...build.matchAll(/xcrun notarytool (\w+)/g)]
    expect(invocations.map(match => match[1]).sort()).toEqual(['history', 'submit'])
    for (const invocation of invocations) {
      const at = invocation.index!
      const materialize = build.lastIndexOf('    materialize_notary_key\n', at)
      const release = build.indexOf('    release_notary_key\n', at)
      expect(materialize).toBeGreaterThan(-1)
      expect(build.slice(materialize, at)).not.toContain('release_notary_key')
      expect(release).toBeGreaterThan(at)
      expect(build.slice(at, release)).not.toContain('materialize_notary_key')
      expect(build.slice(at, release)).not.toMatch(/^\}/m)
    }
    expect(signerInjection).toBeGreaterThan(build.indexOf('echo "=> Creating updater artifacts..."'))
    expect(build).not.toContain('--key "$APPLE_API_KEY_PATH"')

    // 预检入口在解析签名身份与任何构建工具之前退出，不构建也不提交产物
    const preflightExit = build.indexOf('    run_notary_preflight\n    exit $?')
    expect(preflightExit).toBeGreaterThan(keyMaterialization)
    expect(preflightExit).toBeLessThan(build.indexOf('IDENTITIES=$(security find-identity'))
    expect(preflightExit).toBeLessThan(build.indexOf('_RAW_BUNDLE='))
    expect(preflightExit).toBeLessThan(firstBuildTool)
    const preflightFn = build.slice(build.indexOf('run_notary_preflight() {'), preflightExit)
    expect(preflightFn).not.toContain('notarytool submit')

    for (const workflowPath of [
      '../../.github/workflows/nightly.yml',
      '../../.github/workflows/release.yml',
    ]) {
      const workflow = source(workflowPath)
      const buildApp = workflow.indexOf('name: Build app')
      const finalPackage = workflow.indexOf('name: Build widget, sign, package')

      expect(workflow).toContain('APPLE_API_PRIVATE_KEY: ${{ secrets.APPLE_API_PRIVATE_KEY }}')
      expect(workflow).toContain('APPLE_API_KEY: ${{ vars.APPLE_API_KEY }}')
      expect(workflow).toContain('APPLE_API_ISSUER: ${{ vars.APPLE_API_ISSUER }}')
      expect(finalPackage).toBeGreaterThan(buildApp)
      expect(workflow.indexOf('APPLE_API_PRIVATE_KEY:', buildApp)).toBeGreaterThan(finalPackage)
      expect(workflow).toContain('exec src-widget/build.sh')
      expect(workflow).not.toContain('AuthKey_${APPLE_API_KEY}.p8')
      expect(workflow).toContain("trap 'rm -f \"$CERT_FILE\"' EXIT")
      expect(workflow).toContain('unset CERT_P12 CERT_PASSWORD')

      // 预检在 macOS job 的第一个构建步骤之前运行，且只经 build.sh 落地密钥
      const macosJob = workflow.indexOf('\n  build-macos:')
      const preflight = workflow.indexOf('name: Notary preflight')
      const preflightEnd = workflow.indexOf('\n      - ', preflight)
      const preflightStep = workflow.slice(preflight, preflightEnd)
      expect(preflight).toBeGreaterThan(macosJob)
      expect(preflight).toBeLessThan(workflow.indexOf('name: Install dependencies', macosJob))
      expect(preflight).toBeLessThan(workflow.indexOf('name: Import signing certificate', macosJob))
      expect(preflight).toBeLessThan(buildApp)
      expect(preflightStep).toContain('exec src-widget/build.sh --notary-preflight')
      expect(preflightStep).toContain('APPLE_API_PRIVATE_KEY: ${{ secrets.APPLE_API_PRIVATE_KEY }}')
      expect(preflightStep).not.toContain('TAURI_SIGNING_PRIVATE_KEY')
      expect(workflow).not.toContain('notarytool')
      expect(workflow).toContain('notary_preflight_reason: ${{ steps.notary_preflight.outputs.reason }}')
    }
  })

  it('pins every workflow action to an immutable commit SHA', () => {
    for (const workflowPath of [
      '../../.github/workflows/nightly.yml',
      '../../.github/workflows/release.yml',
    ]) {
      const workflow = source(workflowPath)
      const uses = [...workflow.matchAll(/^\s*-?\s*uses:\s*\S+@([^\s#]+)/gm)].map(match => match[1])
      expect(uses.length).toBeGreaterThan(0)
      expect(uses.every(ref => /^[a-f0-9]{40}$/.test(ref))).toBe(true)
    }
  })

  it('requires stable release notes before building and injects them into the updater manifest', () => {
    const releaseScript = source('../../scripts/release.sh')
    const workflow = source('../../.github/workflows/release.yml')
    const manifestScript = source('../../scripts/create-latest-json.mjs')

    expect(releaseScript).toContain('release-notes/v${NEXT_VERSION}.json')
    expect(releaseScript).toContain('REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)')
    expect(releaseScript).toContain('RELEASE_NOTES_FILE="$REPO_ROOT/release-notes/v${NEXT_VERSION}.json"')
    expect(releaseScript).toContain('release-notes.mjs validate')
    expect(workflow).toContain('validate-release-notes:')
    expect(workflow).toContain('RELEASE_NOTES_FILE="$RELEASE_NOTES_FILE" node scripts/create-latest-json.mjs')
    expect(workflow).toContain('--notes "$USER_NOTES"')
    expect(manifestScript).toContain('manifest.notes = encodeReleaseNotes')
  })

  it('publishes a verified Nightly candidate and can restore the previous release', () => {
    const workflow = source('../../.github/workflows/nightly.yml')
    const publisher = source('../../scripts/publish-nightly.sh')
    const candidateUpload = publisher.indexOf('gh release create "$CANDIDATE_TAG"')
    const candidateVerification = publisher.indexOf('verify_release_assets "$CANDIDATE_ID"')
    const oldReleaseBackup = publisher.indexOf('verify_downloaded_assets "$OLD_RELEASE_ID"')
    const oldReleaseDelete = publisher.indexOf('gh release delete nightly', oldReleaseBackup)
    const candidatePublish = publisher.indexOf('-f tag_name=nightly', oldReleaseDelete)
    const publicVerification = publisher.indexOf('verify_release_assets "$PUBLIC_RELEASE_ID"')

    expect(workflow).toContain('bash scripts/publish-nightly.sh')
    expect(workflow).not.toContain('gh release delete nightly --yes --cleanup-tag')
    expect(publisher).toContain('trap rollback ERR')
    expect(publisher).toContain('restoring the previous release')
    expect(candidateUpload).toBeGreaterThan(-1)
    expect(candidateVerification).toBeGreaterThan(candidateUpload)
    expect(oldReleaseBackup).toBeGreaterThan(candidateVerification)
    expect(oldReleaseDelete).toBeGreaterThan(oldReleaseBackup)
    expect(candidatePublish).toBeGreaterThan(oldReleaseDelete)
    expect(publicVerification).toBeGreaterThan(candidatePublish)
    expect(publisher).toContain('expected="sha256:$(shasum -a 256')
    expect(publisher).toContain('gh release create "${RESTORE_ARGS[@]}" "${OLD_ASSETS[@]}"')
    expect(publisher).not.toContain('git push')
  })

  it('keeps Nightly notes tied to commit subjects while Stable requires curated notes', () => {
    const workflow = source('../../.github/workflows/nightly.yml')
    const releaseScript = source('../../scripts/release.sh')

    expect(workflow).not.toContain('CURATED_NOTES_FILE')
    expect(workflow).toContain("git log nightly..HEAD --pretty=format:'%s' -n 8")
    expect(workflow).toContain('release-notes.mjs nightly "$VERSION" "$CHANGES_FILE" "$NOTES_FILE"')
    expect(releaseScript).toContain('release-notes.mjs validate "$RELEASE_NOTES_FILE" "$NEXT_VERSION"')
  })
})
