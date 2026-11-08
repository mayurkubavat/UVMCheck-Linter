// Reference regex results from std::regex (ECMAScript, the default grammar),
// as uvmcheck matches waiver, header and naming patterns. For checking
// dv-lint's patterns (crates/dv-lint/src/pattern.rs, tools/gates/regex_gate.py).
//
// Each input line is PATTERN<TAB>FLAGS<TAB>TEXT, FLAGS empty or "i"; fields
// escape backslash, tab, newline and carriage return as \\ \t \n \r and any
// other byte as \xHH. Each output line is "err" (the pattern does not
// compile), "exc" (matching threw, as libc++ does past its complexity limit),
// or "S M" with S and M 0 or 1: whether regex_search and regex_match succeed.
#include <iostream>
#include <regex>
#include <string>

static std::string unescape(const std::string& s) {
    std::string out;
    for (size_t i = 0; i < s.size(); ++i) {
        if (s[i] != '\\' || i + 1 >= s.size()) {
            out += s[i];
            continue;
        }
        char n = s[++i];
        if (n == 't') out += '\t';
        else if (n == 'n') out += '\n';
        else if (n == 'r') out += '\r';
        else if (n == 'x' && i + 2 < s.size()) {
            out += static_cast<char>(std::stoi(s.substr(i + 1, 2), nullptr, 16));
            i += 2;
        } else out += n;
    }
    return out;
}

int main() {
    std::ios::sync_with_stdio(false);
    std::string line;
    while (std::getline(std::cin, line)) {
        size_t a = line.find('\t'), b = a == std::string::npos ? a : line.find('\t', a + 1);
        if (b == std::string::npos) {
            std::cout << "err\n";
            continue;
        }
        const std::string pattern = unescape(line.substr(0, a)), text = unescape(line.substr(b + 1));
        auto flags = std::regex::ECMAScript;
        if (line.substr(a + 1, b - a - 1) == "i") flags |= std::regex::icase;
        std::regex re;
        try {
            re.assign(pattern, flags);
        } catch (const std::exception&) {
            std::cout << "err\n";
            continue;
        }
        try {
            const bool s = std::regex_search(text, re), m = std::regex_match(text, re);
            std::cout << s << ' ' << m << '\n';
        } catch (const std::exception&) {
            std::cout << "exc\n";
        }
    }
}
