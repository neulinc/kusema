//! Letter-to-sound rules for words no dictionary knows (mostly names).
//!
//! In the style of the NRL rules (Elovitz et al., 1976, public domain),
//! written in Kokoro's (misaki's) American phoneme set. Each rule reads
//! `left[match]right=phonemes`; the first rule for a letter whose contexts
//! fit wins. Context symbols:
//!
//! - ` ` word boundary
//! - `#` one or more vowels, `:` zero or more consonants, `^` one consonant
//! - `.` a voiced consonant, `+` a front vowel (E, I, Y)
//! - `%` a suffix (E, ED, ER, ES, ELY, ING; right side only)
//! - `&` a sibilant, `@` a consonant that makes a following U sound "oo"
//!   (left side only)

use std::collections::HashMap;
use std::sync::LazyLock;

const RULES: &str = r"
 [A] =ə
 [ARE] =ɑɹ
 [AR]O=əɹ
[AR]#=ɛɹ
 ^[AS]#=As
[A]WA=ə
[AW]=ɔ
 :[ANY]=ɛni
[A]^+#=A
#:[ALLY]=əli
 [AL]#=əl
[AGAIN]=əɡɛn
#:[AG]E=ɪʤ
[A]^+:#=æ
 :[A]^+ =A
[A]^%=A
 [ARR]=əɹ
[ARR]=æɹ
 :[AR] =ɑɹ
[AR] =əɹ
[AR]=ɑɹ
[AIR]=ɛɹ
[AI]=A
[AY]=A
[AU]=ɔ
#:[AL] =əl
#:[ALS] =əlz
[ALK]=ɔk
[AL]^=ɔl
 :[ABLE]=Abəl
[ABLE]=əbəl
[ANG]+=Anʤ
[A] =ə
[A]=æ
 [BE]^#=bɪ
[BEING]=biɪŋ
 [BOTH] =bOθ
 [BUS]#=bɪz
[BUIL]=bɪl
[BB]=b
[B]=b
 [CH]^=k
^E[CH]=k
[CH]=ʧ
 S[CI]#=sI
[CI]A=ʃ
[CI]O=ʃ
[CI]EN=ʃ
[C]+=s
[CK]=k
[COM]%=kʌm
[CC]+=ks
[C]=k
#:[DED] =dɪd
.E[D] =d
#:^E[D] =t
 [DE]^#=dɪ
 [DO] =du
 [DOES]=dʌz
 [DOING]=duɪŋ
 [DOW]=dW
[DU]A=ʤu
[DD]=d
[D]=d
#:^[EL] =əl
#:[E] =
'^:[E] =
 :[E] =i
#[ED] =d
#:[E]D =
[EV]ER=ɛv
[E]^%=i
[ERI]#=ɪɹi
[ERI]=ɛɹɪ
#:[ER]#=əɹ
[ER]#=ɛɹ
[ER]=ɜɹ
 [EVEN]=ivən
#:[E]W=
@[EW]=u
[EW]=ju
[E]O=i
#:&[ES] =ɪz
#:[E]S =
#:[ELY] =li
#:[EMENT]=mənt
[EFUL]=fʊl
[EE]=i
[EARN]=ɜɹn
 [EAR]^=ɜɹ
[EAD]=ɛd
#:[EA] =iə
[EA]SU=ɛ
[EA]=i
[EIGH]=A
[EI]=i
 [EYE]=I
[EY]=i
[EU]=ju
[E]=ɛ
[FUL]=fʊl
[FF]=f
[F]=f
[GIV]=ɡɪv
 [G]I^=ɡ
[GE]T=ɡɛ
SU[GGES]=ɡʤɛs
[GG]=ɡ
 B#[G]=ɡ
[G]+=ʤ
[GREAT]=ɡɹAt
#[GH]=
 [GN]=n
[G]=ɡ
 [HAV]=hæv
 [HERE]=hiɹ
 [HOUR]=Wəɹ
[HOW]=hW
[H]#=h
[H]=
 [IN]=ɪn
 [I] =I
[IN]D=In
[IER]=iəɹ
#:R[IED] =id
[IED] =Id
[IEN]=iɛn
[IE]T=Iɛ
 :[I]%=I
[I]%=i
[IE]=i
[I]^+:#=ɪ
[IR]#=Iɹ
[IZ]%=Iz
[IS]%=Iz
[I]D%=I
+^[I]^+=ɪ
[I]T%=I
#:^[I]^+=ɪ
[I]^+=I
[IR]=ɜɹ
[IGH]=I
[ILD]=Ild
[IGN] =In
[IGN]^=In
[IGN]%=In
[IQUE]=ik
[I]A=i
[I]=ɪ
[J]=ʤ
 [K]N=
[KK]=k
[K]=k
[LO]C#=lO
L[L]=
#:^[L]%=əl
[LEAD]=lid
[L]=l
[MOV]=muv
[MM]=m
[M]=m
E[NG]+=nʤ
[NG]R=ŋɡ
[NG]#=ŋɡ
[NGL]%=ŋɡəl
[NG]=ŋ
[NK]=ŋk
 [NOW] =nW
[NN]=n
[N]=n
[OF] =əv
[OROUGH]=ɜɹO
#:[OR] =əɹ
#:[ORS] =əɹz
[OR]=ɔɹ
 [ONE]=wʌn
[OW]=O
 [OVER]=Ovəɹ
[OV]=ʌv
[O]^%=O
[O]^EN=O
[O]^I#=O
[OL]D=Ol
[OUGHT]=ɔt
[OUGH]=ʌf
 [OU]=W
H[OU]S#=W
[OUS]=əs
[OUR]=ɔɹ
[OULD]=ʊd
^[OU]^L=ʌ
[OUP]=up
[OU]=W
[OY]=Y
[OING]=Oɪŋ
[OI]=Y
[OOR]=ɔɹ
[OOK]=ʊk
[OOD]=ʊd
[OO]=u
[O]E=O
[O] =O
[OA]=O
 [ONLY]=Onli
 [ONCE]=wʌns
[ON'T]=Ont
C[O]N=ɑ
[O]NG=ɔ
 :^[O]N=ʌ
I[ON]=ən
#:[ON] =ən
#^[ON]=ən
[O]ST =O
[OF]^=ɔf
[OTHER]=ʌðəɹ
[OSS] =ɔs
#:^[OM]=ʌm
[O]=ɑ
[PH]=f
[PEOP]=pip
[POW]=pW
[PUT] =pʊt
[PP]=p
 [PS]=s
 [PN]=n
[P]=p
[QUAR]=kwɔɹ
[QUE] =k
[QU]=kw
[Q]=k
 [RE]^#=ɹi
[RH]=ɹ
[RR]=ɹ
[R]=ɹ
[SH]=ʃ
#[SION]=ʒən
[SOME]=sʌm
#[SUR]#=ʒəɹ
[SUR]#=ʃəɹ
#[SU]#=ʒu
#[SSU]#=ʃu
#[SED] =zd
#[S]#=z
[SAID]=sɛd
^[SION]=ʃən
[S]S=
.[S] =z
#:.E[S] =z
#:^##[S] =z
#:^#[S] =s
U[S] =s
 :#[S] =z
 [SCH]=sk
[S]C+=
#[SM]=zəm
[S]=s
 [THE] =ðə
[TO] =tu
[THAT] =ðæt
 [THIS] =ðɪs
 [THEY]=ðA
 [THERE]=ðɛɹ
[THER]=ðəɹ
[THEIR]=ðɛɹ
 [THAN] =ðæn
 [THEM] =ðɛm
[THESE] =ðiz
 [THEN]=ðɛn
[THROUGH]=θɹu
[THOSE]=ðOz
[THOUGH] =ðO
 [THUS]=ðʌs
[TH]=θ
#:[TED] =tɪd
S[TI]#N=ʧ
[TI]O=ʃ
[TI]A=ʃ
[TIEN]=ʃən
[TUR]#=ʧəɹ
[TU]A=ʧu
 [TWO]=tu
[TCH]=ʧ
[TT]=t
[T]=t
 [UN]I=jun
 [UN]=ʌn
 [UPON]=əpɔn
@[UR]#=ʊɹ
[UR]#=jʊɹ
[UR]=ɜɹ
[U]^ =ʌ
[U]^^=ʌ
[UY]=I
 G[U]#=
G[U]%=
G[U]#=w
#N[U]=ju
@[U]=u
[U]=ju
[VV]=v
[V]=v
 [WERE]=wɜɹ
[WAS] =wʌz
[WAT]=wɑt
[WHERE]=wɛɹ
[WHAT]=wʌt
[WHOL]=hOl
[WHO]=hu
[WH]=w
[WAR]=wɔɹ
[WOR]^=wɜɹ
[WR]=ɹ
[W]=w
 [X]=z
[X]=ks
[YOUNG]=jʌŋ
 [YOU]=ju
 [YES]=jɛs
 [Y]=j
#:^[Y] =i
#:^[Y]I=i
 :[Y] =I
 :[Y]#=I
 :[Y]^+:#=ɪ
 :[Y]^#=I
[Y]=ɪ
[ZZ]=z
[Z]=z
";

struct Rule {
    left: Vec<char>,
    matched: Vec<char>,
    right: Vec<char>,
    phonemes: &'static str,
}

/// Rules grouped by the first letter they match, in order.
static TABLE: LazyLock<HashMap<char, Vec<Rule>>> = LazyLock::new(|| {
    let mut table: HashMap<char, Vec<Rule>> = HashMap::new();
    for line in RULES.lines().filter(|l| l.contains('[')) {
        let (pattern, phonemes) = line.split_once('=').expect("rule has '='");
        let (left, rest) = pattern.split_once('[').expect("rule has '['");
        let (matched, right) = rest.split_once(']').expect("rule has ']'");
        let rule = Rule {
            left: left.chars().collect(),
            matched: matched.chars().collect(),
            right: right.chars().collect(),
            phonemes,
        };
        table.entry(rule.matched[0]).or_default().push(rule);
    }
    table
});

fn is_vowel(c: char) -> bool {
    matches!(c, 'A' | 'E' | 'I' | 'O' | 'U' | 'Y')
}

fn is_consonant(c: char) -> bool {
    c.is_ascii_uppercase() && !is_vowel(c)
}

fn is_voiced(c: char) -> bool {
    matches!(
        c,
        'B' | 'D' | 'G' | 'J' | 'L' | 'M' | 'N' | 'R' | 'V' | 'W' | 'Z'
    )
}

/// Match `pattern` going right from `pos` (exclusive of the matched letters).
fn match_right(word: &[char], mut pos: usize, pattern: &[char]) -> bool {
    let mut i = 0;
    while i < pattern.len() {
        let at = word.get(pos).copied().unwrap_or(' ');
        match pattern[i] {
            '#' => {
                if !is_vowel(at) {
                    return false;
                }
                while word.get(pos).copied().is_some_and(is_vowel) {
                    pos += 1;
                }
                i += 1;
                continue;
            }
            ':' => {
                while word.get(pos).copied().is_some_and(is_consonant) {
                    pos += 1;
                }
                i += 1;
                continue;
            }
            '^' if !is_consonant(at) => return false,
            '.' if !is_voiced(at) => return false,
            '+' if !matches!(at, 'E' | 'I' | 'Y') => return false,
            // A suffix that ends the word (always last in a pattern).
            '%' => {
                let rest: String = word[pos.min(word.len())..].iter().collect();
                return ["ING", "ELY", "ER", "ES", "ED", "E"].contains(&rest.as_str());
            }
            '^' | '.' | '+' => {}
            c if c != at => return false,
            _ => {}
        }
        pos += 1;
        i += 1;
    }
    true
}

/// Match `pattern` going left from just before `pos`.
fn match_left(word: &[char], pos: usize, pattern: &[char]) -> bool {
    let mut pos = pos as isize - 1;
    let at = |p: isize| {
        if p < 0 {
            ' '
        } else {
            word.get(p as usize).copied().unwrap_or(' ')
        }
    };
    for &symbol in pattern.iter().rev() {
        let c = at(pos);
        match symbol {
            '#' => {
                if !is_vowel(c) {
                    return false;
                }
                while is_vowel(at(pos)) {
                    pos -= 1;
                }
                continue;
            }
            ':' => {
                while is_consonant(at(pos)) {
                    pos -= 1;
                }
                continue;
            }
            '^' if !is_consonant(c) => return false,
            '.' if !is_voiced(c) => return false,
            '+' if !matches!(c, 'E' | 'I' | 'Y') => return false,
            '&' => {
                let pair = (at(pos - 1), c);
                if matches!(pair, ('C', 'H') | ('S', 'H')) {
                    pos -= 1;
                } else if !matches!(c, 'S' | 'C' | 'G' | 'Z' | 'X' | 'J') {
                    return false;
                }
            }
            '@' => {
                let pair = (at(pos - 1), c);
                if matches!(pair, ('T', 'H') | ('C', 'H') | ('S', 'H')) {
                    pos -= 1;
                } else if !matches!(c, 'T' | 'S' | 'R' | 'D' | 'L' | 'Z' | 'N' | 'J') {
                    return false;
                }
            }
            '^' | '.' | '+' => {}
            s if s != c => return false,
            _ => {}
        }
        pos -= 1;
    }
    true
}

/// Letters with accents → plain ASCII letters the rules know.
fn fold(c: char) -> Option<char> {
    let c = c.to_ascii_uppercase();
    if c.is_ascii_uppercase() || c == '\'' {
        return Some(c);
    }
    let base = match c.to_lowercase().next()? {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'A',
        'ç' | 'č' => 'C',
        'è' | 'é' | 'ê' | 'ë' | 'ē' => 'E',
        'ì' | 'í' | 'î' | 'ï' => 'I',
        'ñ' => 'N',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'O',
        'ù' | 'ú' | 'û' | 'ü' => 'U',
        'ý' | 'ÿ' => 'Y',
        'š' => 'S',
        'ž' => 'Z',
        _ => return None,
    };
    Some(base)
}

/// Pronounce a word by rule, with primary stress on the first vowel.
pub fn pronounce(word: &str) -> String {
    let letters: Vec<char> = word.chars().filter_map(fold).collect();
    let mut out = String::new();
    let mut pos = 0;
    while pos < letters.len() {
        let c = letters[pos];
        let rule = TABLE.get(&c).and_then(|rules| {
            rules.iter().find(|r| {
                letters[pos..].starts_with(&r.matched)
                    && match_left(&letters, pos, &r.left)
                    && match_right(&letters, pos + r.matched.len(), &r.right)
            })
        });
        match rule {
            Some(r) => {
                out.push_str(r.phonemes);
                pos += r.matched.len();
            }
            None => pos += 1,
        }
    }
    stress(&out)
}

const VOWELS: &str = "AIOWYaeiouæɑɔəɛɜɪʊʌ";

fn stress(phonemes: &str) -> String {
    match phonemes.char_indices().find(|(_, c)| VOWELS.contains(*c)) {
        Some((i, _)) if phonemes.chars().filter(|c| VOWELS.contains(*c)).count() > 1 => {
            format!("{}ˈ{}", &phonemes[..i], &phonemes[i..])
        }
        _ => phonemes.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_parses() {
        let count: usize = TABLE.values().map(Vec::len).sum();
        assert!(count > 200);
        // Every letter has a catch-all rule, so no letter is dropped.
        for c in 'A'..='Z' {
            let rules = &TABLE[&c];
            assert!(
                rules
                    .iter()
                    .any(|r| r.matched == [c] && r.left.is_empty() && r.right.is_empty()),
                "{c} has no default rule"
            );
        }
    }

    #[test]
    fn common_patterns() {
        assert_eq!(pronounce("cat"), "kæt");
        assert_eq!(pronounce("ship"), "ʃɪp");
        assert_eq!(pronounce("make"), "mAk");
        assert_eq!(pronounce("phone"), "fOn");
        assert_eq!(pronounce("night"), "nIt");
        assert_eq!(pronounce("Mabel"), "mˈæbəl");
        assert_eq!(pronounce("Zorblax"), "zˈɔɹblæks");
    }
}
