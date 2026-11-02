# Releasing

Releases are cut by [release-plz](https://release-plz.dev): merging its release
PR tags `vX.Y.Z`, publishes to crates.io, and the `Release assets` job attaches
binaries, the CycloneDX SBOM and their build-provenance attestations. CI cannot
sign with the maintainer's YubiKey, so the last step is manual.

## Sign the release

1. Download the assets:

   ```sh
   tag=vX.Y.Z
   mkdir "rel-$tag" && cd "rel-$tag"
   gh release download "$tag" --repo asakura/inceptool \
     --pattern '*.tar.gz' --pattern '*.cdx.json'
   ```

2. Verify CI provenance first, so a tampered asset is never endorsed:

   ```sh
   for f in *.tar.gz *.cdx.json; do
     gh attestation verify "$f" --repo asakura/inceptool \
       --signer-workflow asakura/inceptool/.github/workflows/release-binaries.yml
   done
   ```

3. Create and sign the checksum file (touch the YubiKey):

   ```sh
   sha256sum *.tar.gz *.cdx.json > SHA256SUMS
   ssh-keygen -Y sign -f ~/.ssh/id_ed25519_sk -n inceptool-release SHA256SUMS
   ```

4. Check the signature locally, then upload:

   ```sh
   ssh-keygen -Y verify -f ../allowed_signers -I inceptool-release \
     -n inceptool-release -s SHA256SUMS.sig < SHA256SUMS
   gh release upload "$tag" SHA256SUMS SHA256SUMS.sig --repo asakura/inceptool
   ```

## Signing key

A resident FIDO2 key on the YubiKey, created once with:

```sh
ssh-keygen -t ed25519-sk -O resident -O verify-required -C inceptool-release
```

The public key is committed in [`allowed_signers`](allowed_signers). To rotate,
generate a new key, append it with a validity window, and keep the old line so
past releases still verify.
