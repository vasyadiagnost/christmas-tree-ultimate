Audio assets for christmas tree ultimate

Santa: Sleigh Bells by MissyWhimsyArt (Lynda Smith-McDaniel).
Author's listing: https://pixabay.com/sound-effects/film-special-effects-sleigh-bells-272151/
Original recording used for this build:
https://audio.com/azure-reef/audio/sleigh-bells-by-missywhimsyart-royalty-free-music-151272
Content terms: https://pixabay.com/service/terms/

To prepare the asset for a source build, follow RELEASING.md at the repository
root. The script takes the original MP3 as input and produces santa-bells.wav.
It removes 40 ms of lead-in and 32.653 ms of tail, with a 40 ms equal-power
blend. The resulting loop is 6 s, mono, 44,100 Hz, 16-bit PCM. The verified
0.2.2 embedded WAV SHA-256 is:
552A25BE50820311ABB9644CED9253EDBF7D839F8319BB407D14B77771051456

tractor-snow.wav is the original procedural engine and snow-brushing loop.
scripts/generate-audio.py regenerates only that eight-second asset.
