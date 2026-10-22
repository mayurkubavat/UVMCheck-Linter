#include "uvmcheck/core/Json.h"
#include <charconv>
#include <cstdint>

namespace uvmcheck::core::json {

const Value* Value::find(std::string_view key) const {
    if (!isObject()) return nullptr;
    const auto& obj = asObject();
    auto it = obj.find(key);
    return it == obj.end() ? nullptr : &it->second;
}

const char* Value::typeName() const {
    if (isNull()) return "null";
    if (isBool()) return "boolean";
    if (isNumber()) return "number";
    if (isString()) return "string";
    if (isArray()) return "array";
    return "object";
}

namespace {

class Parser {
public:
    Parser(std::string_view text, ParseError& error) : text(text), error(error) {}

    std::optional<Value> parseDocument() {
        auto v = parseValue(0);
        if (!v) return std::nullopt;
        skipWhitespace();
        if (pos != text.size()) return fail("unexpected trailing characters");
        return v;
    }

private:
    static constexpr int MaxDepth = 256;

    std::string_view text;
    ParseError& error;
    size_t pos = 0;
    bool failed = false;

    std::nullopt_t fail(std::string message) {
        if (!failed) {
            failed = true;
            error.message = std::move(message);
            error.line = 1;
            error.column = 1;
            for (size_t i = 0; i < pos && i < text.size(); ++i) {
                if (text[i] == '\n') {
                    error.line++;
                    error.column = 1;
                } else {
                    error.column++;
                }
            }
        }
        return std::nullopt;
    }

    void skipWhitespace() {
        while (pos < text.size()) {
            char c = text[pos];
            if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                pos++;
            } else if (c == '/' && pos + 1 < text.size() && text[pos + 1] == '/') {
                while (pos < text.size() && text[pos] != '\n') pos++;
            } else if (c == '/' && pos + 1 < text.size() && text[pos + 1] == '*') {
                size_t end = text.find("*/", pos + 2);
                pos = (end == std::string_view::npos) ? text.size() : end + 2;
            } else {
                break;
            }
        }
    }

    bool consumeLiteral(std::string_view lit) {
        if (text.substr(pos, lit.size()) == lit) {
            pos += lit.size();
            return true;
        }
        return false;
    }

    std::optional<Value> parseValue(int depth) {
        if (depth > MaxDepth) return fail("nesting too deep");
        skipWhitespace();
        if (pos >= text.size()) return fail("unexpected end of input");

        char c = text[pos];
        if (c == '{') return parseObject(depth);
        if (c == '[') return parseArray(depth);
        if (c == '"') {
            auto s = parseString();
            if (!s) return std::nullopt;
            return Value(std::move(*s));
        }
        if (consumeLiteral("true")) return Value(true);
        if (consumeLiteral("false")) return Value(false);
        if (consumeLiteral("null")) return Value();
        if (c == '-' || (c >= '0' && c <= '9')) return parseNumber();
        return fail(std::string("unexpected character '") + c + "'");
    }

    std::optional<Value> parseObject(int depth) {
        pos++; // '{'
        Value::Object obj;
        while (true) {
            skipWhitespace();
            if (pos < text.size() && text[pos] == '}') {
                pos++;
                return Value(std::move(obj));
            }
            if (pos >= text.size() || text[pos] != '"') return fail("expected object key string");
            auto key = parseString();
            if (!key) return std::nullopt;
            skipWhitespace();
            if (pos >= text.size() || text[pos] != ':') return fail("expected ':' after object key");
            pos++;
            auto val = parseValue(depth + 1);
            if (!val) return std::nullopt;
            obj[std::move(*key)] = std::move(*val);
            skipWhitespace();
            if (pos < text.size() && text[pos] == ',') {
                pos++;
            } else if (pos < text.size() && text[pos] == '}') {
                pos++;
                return Value(std::move(obj));
            } else {
                return fail("expected ',' or '}' in object");
            }
        }
    }

    std::optional<Value> parseArray(int depth) {
        pos++; // '['
        Value::Array arr;
        while (true) {
            skipWhitespace();
            if (pos < text.size() && text[pos] == ']') {
                pos++;
                return Value(std::move(arr));
            }
            auto val = parseValue(depth + 1);
            if (!val) return std::nullopt;
            arr.push_back(std::move(*val));
            skipWhitespace();
            if (pos < text.size() && text[pos] == ',') {
                pos++;
            } else if (pos < text.size() && text[pos] == ']') {
                pos++;
                return Value(std::move(arr));
            } else {
                return fail("expected ',' or ']' in array");
            }
        }
    }

    std::optional<Value> parseNumber() {
        size_t start = pos;
        if (text[pos] == '-') pos++;
        while (pos < text.size() &&
               ((text[pos] >= '0' && text[pos] <= '9') || text[pos] == '.' ||
                text[pos] == 'e' || text[pos] == 'E' || text[pos] == '+' || text[pos] == '-')) {
            pos++;
        }
        double d = 0;
        auto [ptr, ec] = std::from_chars(text.data() + start, text.data() + pos, d);
        if (ec != std::errc() || ptr != text.data() + pos) {
            pos = start;
            return fail("invalid number");
        }
        return Value(d);
    }

    static void appendUtf8(std::string& out, uint32_t cp) {
        if (cp < 0x80) {
            out += static_cast<char>(cp);
        } else if (cp < 0x800) {
            out += static_cast<char>(0xC0 | (cp >> 6));
            out += static_cast<char>(0x80 | (cp & 0x3F));
        } else if (cp < 0x10000) {
            out += static_cast<char>(0xE0 | (cp >> 12));
            out += static_cast<char>(0x80 | ((cp >> 6) & 0x3F));
            out += static_cast<char>(0x80 | (cp & 0x3F));
        } else {
            out += static_cast<char>(0xF0 | (cp >> 18));
            out += static_cast<char>(0x80 | ((cp >> 12) & 0x3F));
            out += static_cast<char>(0x80 | ((cp >> 6) & 0x3F));
            out += static_cast<char>(0x80 | (cp & 0x3F));
        }
    }

    std::optional<uint32_t> parseHex4() {
        if (pos + 4 > text.size()) return std::nullopt;
        uint32_t cp = 0;
        auto [ptr, ec] = std::from_chars(text.data() + pos, text.data() + pos + 4, cp, 16);
        if (ec != std::errc() || ptr != text.data() + pos + 4) return std::nullopt;
        pos += 4;
        return cp;
    }

    std::optional<std::string> parseString() {
        pos++; // opening quote
        std::string out;
        while (pos < text.size()) {
            char c = text[pos++];
            if (c == '"') return out;
            if (c == '\n') return fail("unterminated string");
            if (c != '\\') {
                out += c;
                continue;
            }
            if (pos >= text.size()) break;
            char e = text[pos++];
            switch (e) {
                case '"': out += '"'; break;
                case '\\': out += '\\'; break;
                case '/': out += '/'; break;
                case 'b': out += '\b'; break;
                case 'f': out += '\f'; break;
                case 'n': out += '\n'; break;
                case 'r': out += '\r'; break;
                case 't': out += '\t'; break;
                case 'u': {
                    auto cp = parseHex4();
                    if (!cp) return fail("invalid \\u escape");
                    if (*cp >= 0xD800 && *cp <= 0xDBFF && text.substr(pos, 2) == "\\u") {
                        pos += 2;
                        auto lo = parseHex4();
                        if (!lo || *lo < 0xDC00 || *lo > 0xDFFF) return fail("invalid surrogate pair");
                        *cp = 0x10000 + ((*cp - 0xD800) << 10) + (*lo - 0xDC00);
                    }
                    appendUtf8(out, *cp);
                    break;
                }
                default:
                    pos--;
                    return fail(std::string("invalid escape '\\") + e + "'");
            }
        }
        return fail("unterminated string");
    }
};

} // namespace

std::optional<Value> parse(std::string_view text, ParseError& error) {
    Parser p(text, error);
    return p.parseDocument();
}

} // namespace uvmcheck::core::json
