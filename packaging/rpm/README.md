# Building the RPM

```
packaging/rpm/make-vendor-tarball.sh          # both sources, into ~/rpmbuild/SOURCES
rpmbuild -ba packaging/rpm/sam.spec           # → ~/rpmbuild/RPMS/x86_64/sam-<version>-1.fc44.x86_64.rpm
sudo dnf install ~/rpmbuild/RPMS/x86_64/sam-<version>-1.fc44.x86_64.rpm
sam setup                                     # as your user: Super+Space and the service
```

- **The source tarball is `git archive HEAD`.** Commit before building; the
  script warns if the tree is dirty.
- **The build is offline.** The vendor tarball carries every crate in
  `Cargo.lock`, together with the source replacement that points cargo at
  them.
- **`%check` runs `sam-core`'s tests only.** The adapter's tests need a
  desktop session.
- **The package installs these files:**
  - the binary
  - the desktop file
  - the D-Bus service
  - the systemd user unit, in `%{_userunitdir}`
  - the icon

  It changes nothing per user. `sam setup` binds the key and enables the
  unit; `sam setup --undo` reverses both.
- **A development install in `~/.local` takes precedence over the RPM's
  files** for the unit, the D-Bus service and the desktop file. Remove it
  with `scripts/install-user.sh --uninstall` before relying on the package.
