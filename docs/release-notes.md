# Burntime Remastered 1.3 for AROS, port release 1

A port of Jakob Harder's Burntime Remastered 1.3 to AROS, rewritten in Rust.
Burntime is Max Design's 1993 strategy game of survival after the bomb. This
release plays the remaster's 1.3 rules (Classic and Extended, one or two
players, the Modern, Amiga or no machine opponents, New Village) in the
original graphics or the remaster's.

No game data is included. You need your own copy of Burntime and the
remaster's files to make `burntime.pack` with the packer from the source
archive. The `ReadMe` in each archive explains how.

## Downloads

- `Burntime-1.3-aros1-aros-x86_64-abiv11-pc.zip` and `.apkg.zip`: AROS One and other x86_64 ABIv11 systems.
- `Burntime-1.3-aros1-aros-x86_64-abiv1-pc.zip` and `.apkg.zip`: AROS mainline (ABIv1) on x86_64 PCs.
- `Burntime-1.3-aros1-aros-aarch64-abiv1-raspi.zip`: AROS on the Raspberry Pi. Unpack it on another computer and copy the drawer to the SD card.
- `Burntime-1.3-aros1-source.zip`: the source, with the packer and the build instructions.
- `SHA256SUMS`

The `.apkg.zip` files install with apkg or PkgManager (arospkg), which show the version as `1.3-aros1`; arospkg 0.7.4 or later is recommended.

## Differences from Burntime Remastered 1.3

- Saves are in this program's own format; the remaster's saves cannot be loaded.
- The picture is drawn without the remaster's shaders and scaled to the window.
- The game advances in fixed steps of 1/18 second.
- The window opens larger than half the screen where half the screen would cut off the game.
- Where the remaster would stop with an error (building on the world map with a part missing, among others), this program goes on.
- On AROS mainline and the Raspberry Pi the game runs at task priority -1, because at the normal priority its sound stopped for seconds at a time. It sets the previous priority back when it ends.
- Touch controls and the remaster's experimental character pictures are not included.

## Known limitations

- On the Raspberry Pi, `Avail FLUSH`, or a program that asks for more memory than is free, makes AROS stop with a Software Failure (Illegal instruction, PC 0x8). This is an AROS defect in its USB controller driver (https://github.com/aros-development-team/AROS/issues/2068); a fix is proposed in https://github.com/aros-development-team/AROS/pull/2104. Do not use `Avail FLUSH` on the Pi. If it happens, restart AROS.
- On the Raspberry Pi, a Software Failure (Illegal instruction) has also appeared twice while the game was running, both times in QEMU's Raspberry Pi emulation after many games in one session. Its cause is not known. It shows the same signature as the defect above, but that does not show that it has the same cause, and the proposed fix for that defect is not known to prevent it. If it happens, restart AROS. Save your game often when you play on the Pi. About 137 MB of memory stay in use after the first game until the next restart.
- There is no package for the Raspberry Pi: a drawer that apkg installs on the Pi's system partition makes AROS ask for a volume "Burntime" when the game starts (an AROS FAT defect). Install by copying the drawer.
- The game has not been tried on a physical Raspberry Pi, with sound on a Pi, or with a game controller.
- There is no AROS version of the packer.

## Licence and credits

MIT, Tomasz Staniak. The parts translated from Burntime Remastered are under its MIT licence, Jakob Harder. The program contains GNU libiconv (LGPL 2.1 or later); `NOTICE` says where its source is. Burntime (1993) is by Max Design; its graphics, music, sounds and texts are not included. `NOTICE` in each archive lists the libraries and the credits.
