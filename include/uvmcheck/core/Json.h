#pragma once

#include <map>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <variant>
#include <vector>

namespace uvmcheck::core::json {

/// Minimal JSON DOM used for configuration files. Objects keep their keys
/// sorted (std::map); duplicate keys keep the last value, as most parsers do.
class Value {
public:
    using Array = std::vector<Value>;
    using Object = std::map<std::string, Value, std::less<>>;

    Value() = default;
    explicit Value(bool b) : data(b) {}
    explicit Value(double d) : data(d) {}
    explicit Value(std::string s) : data(std::move(s)) {}
    explicit Value(Array a) : data(std::make_shared<Array>(std::move(a))) {}
    explicit Value(Object o) : data(std::make_shared<Object>(std::move(o))) {}

    bool isNull() const { return std::holds_alternative<std::monostate>(data); }
    bool isBool() const { return std::holds_alternative<bool>(data); }
    bool isNumber() const { return std::holds_alternative<double>(data); }
    bool isString() const { return std::holds_alternative<std::string>(data); }
    bool isArray() const { return std::holds_alternative<std::shared_ptr<Array>>(data); }
    bool isObject() const { return std::holds_alternative<std::shared_ptr<Object>>(data); }

    bool asBool() const { return std::get<bool>(data); }
    double asNumber() const { return std::get<double>(data); }
    const std::string& asString() const { return std::get<std::string>(data); }
    const Array& asArray() const { return *std::get<std::shared_ptr<Array>>(data); }
    const Object& asObject() const { return *std::get<std::shared_ptr<Object>>(data); }

    /// Object member lookup; returns nullptr when this is not an object or the key is missing.
    const Value* find(std::string_view key) const;

    /// Human-readable type name for error messages.
    const char* typeName() const;

private:
    std::variant<std::monostate, bool, double, std::string,
                 std::shared_ptr<Array>, std::shared_ptr<Object>> data;
};

struct ParseError {
    std::string message;
    size_t line = 0;
    size_t column = 0;
};

/// Parses a JSON document. Accepts `//` and `/* */` comments and trailing
/// commas so that hand-edited config files stay forgiving.
std::optional<Value> parse(std::string_view text, ParseError& error);

} // namespace uvmcheck::core::json
