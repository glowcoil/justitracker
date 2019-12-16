# Justi

Justi is a just intonation tracker. 



## Usage

Each column represents a track.

The sets of numbers in each step represent the tuning for that step.

```
00 00 00 00
```
These four sets of numbers respectively represent:
- Octaves (Factor of 2)

- Fifths (Factor of 3/2)

- Major 3rds (Factor of 5/4)

- Minor 7ths (Factor of 7/4)

  

So four sets of 00 means the sample is played at its recorded pitch, 
`01 00 00 00` means the sample is played an octave above,
`-01 01 00 00` means the sample is down an octave and up a fifth, which works out to be down a fourth.



### Controls

- Arrow Keys to move around
- 1, 2, 3, 4 to move the note in the current cell up
- Shift + 1, 2, 3, 4 move the note in the current cell down 
  - (Note movement is by a factor of 2, 3/2, 5/4, and 7/4. See Usage for more info)
- Delete/Backspace to remove note
- Backtick/Tilde to insert a note-off
- i to load a wav file for the current track's instrument
- Escape to Quit
- Space (or click the Play Button) to Play/Stop



Currently Justi does not support Saving/Loading or any Export/Import