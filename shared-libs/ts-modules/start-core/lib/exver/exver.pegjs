// #flavor:0.1.2-beta.1:0
// !( >=1:1 && <= 2:2)

VersionRange
  = first:VersionRangeAtom rest:(_ ((Or / And) _)? VersionRangeAtom)*

Or = "||"

And = "&&"

VersionRangeAtom
  = Parens
  / Anchor
  / Not
  / Any
  / None
  / FlavorAtom

Parens
  = "(" _ expr:VersionRange _ ")" { return { type: "Parens", expr } }

Anchor
  = operator:CmpOp? _ version:VersionSpec { return { type: "Anchor", operator, version } }

VersionSpec
  = flavor:Flavor? upstream:Version downstream:( ":" Version )? { return { flavor: flavor || null, upstream, downstream: downstream ? downstream[1] : { number: [0], prerelease: [] } } }

FlavorAtom
  = "#" flavor:FlavorString { return { type: "Flavor", flavor: flavor } }

Not = "!" _ value:VersionRangeAtom { return { type: "Not", value: value }}

Any = "*" { return { type: "Any" } }

None = "!" { return { type: "None" } }

CmpOp
  = ">=" { return ">="; }
  / "<=" { return "<="; }
  / ">" { return ">"; }
  / "<" { return "<"; }
  / "=" { return "="; }
  / "!=" { return "!="; }
  / "^" { return "^"; }
  / "~" { return "~"; }

ExtendedVersion
  = flavor:Flavor? upstream:Version ":" downstream:Version {
    return { flavor: flavor || null, upstream, downstream }
  }

EmverVersionRange
  = first:EmverVersionRangeAtom rest:(_ ((Or / And) _)? EmverVersionRangeAtom)*

EmverVersionRangeAtom
  = EmverParens
  / EmverAnchor
  / EmverNot
  / Any
  / None

EmverParens
  = "(" _ expr:EmverVersionRange _ ")" { return { type: "Parens", expr } }

EmverAnchor
  = operator:CmpOp? _ version:Emver { return { type: "Anchor", operator, version } }

EmverNot = "!" _ value:EmverVersionRangeAtom { return { type: "Not", value: value }}

Emver
  = major:Digit "." minor:Digit "." patch:Digit revision:( "." revision:Digit { return revision } )? {
    return {
      flavor: null,
      upstream: {
        number: [major, minor, patch],
        prerelease: [],
      },
      downstream: {
        number: [revision || 0],
        prerelease: [],
      },
    }
  }

Flavor
  = "#" flavor:FlavorString ":" { return flavor }

FlavorString
  = [a-z]* { return text() }

Version
  = number:VersionNumber prerelease: PreRelease? {
    return {
      number,
      prerelease: prerelease || []
    };
  }

PreRelease
  = "-" first:PreReleaseSegment rest:("." PreReleaseSegment)* {
    return [first].concat(rest.map(r => r[1]));
  }

PreReleaseSegment
  = segment:$([0-9a-zA-Z-]+) &{ return !/^0[0-9]+$/.test(segment); } {
    if (!/^[0-9]+$/.test(segment)) return segment;
    const value = Number(segment);
    return Number.isSafeInteger(value) ? value : segment;
  }

VersionNumber
  = first:Digit rest:("." Digit)* {
    return [first].concat(rest.map(r => r[1]));
  }

Digit
  = [0-9]+ { return parseInt(text(), 10); }

_ "whitespace"
  = [ \t\n\r]*
