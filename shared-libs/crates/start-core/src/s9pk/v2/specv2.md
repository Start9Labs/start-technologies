## Magic

`0x3b3b`

## Version

`0x02` (varint)

## Merkle Archive

### Header

- ed25519 pubkey (32B)
- ed25519 signature of TOC sighash (64B)
- TOC sighash: (32B)
- TOC position: (8B: u64 BE)
- TOC size: (8B: u64 BE)

### TOC

- number of entries (varint)
- FOREACH section
  - name (varstring)
  - hash (32B: BLAKE-3 of file contents / TOC sighash)
  - TYPE (1B)
    - TYPE=MISSING (`0x00`)
    - TYPE=FILE (`0x01`)
      - position (8B: u64 BE)
      - size (8B: u64 BE)
    - TYPE=TOC (`0x02`)
      - position (8B: u64 BE)
      - size (8B: u64 BE)

#### SigHash

Hash of TOC with all contents MISSING

### FILE

`<File contents>`

# Example

`foo/bar/baz.txt`

ROOT TOC:

- 1 section
  - name: foo
    hash: sighash('a)
    type: TOC
    position: 'a
    length: \_

'a:

- 1 section
  - name: bar
    hash: sighash('b)
    type: TOC
    position: 'b
    size: \_

'b:

- 2 sections
  - name: baz.txt
    hash: hash('c)
    type: FILE
    position: 'c
    length: \_
  - name: qux
    hash: `<unverifiable>`
    type: MISSING

'c: `<CONTENTS OF baz.txt>`

"foo/"
hash: \_
size: 15b

"bar.txt"
hash: \_
size: 5b

`<CONTENTS OF foo/>` (
"baz.txt"
hash: \_
size: 2b
)
`<CONTENTS OF bar.txt>` ("hello")
`<CONTENTS OF baz.txt>` ("hi")
