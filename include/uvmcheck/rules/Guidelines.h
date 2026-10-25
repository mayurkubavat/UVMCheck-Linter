#pragma once

// The UVM methodology guide (docs/methodology/): chapters MG-1 to MG-9, the guidelines in
// them, and the rules that enforce each guideline. The tables live in one place so the CLI
// (--guideline, --list-guidelines), the reports and the docs test all read the same mapping.

#include <string_view>
#include <vector>

namespace uvmcheck::rules {

/// Guideline levels use the wording of standards documents.
enum class GuidelineLevel {
    Shall,  // requirement; its rules report warnings or errors and run by default
    Should, // recommendation; may be departed from with a waiver that says why
    May     // advice; its rules are usually Advisory
};

struct Guideline {
    std::string_view id;           // "MG-4.1"
    std::string_view chapter;      // "MG-4"
    std::string_view chapterTitle; // "Reusable verification components"
    GuidelineLevel level;
    std::string_view statement;
};

const char* levelName(GuidelineLevel level);

/// Every guideline, in guide order.
const std::vector<Guideline>& methodologyGuidelines();

/// The guideline with this ID, or nullptr.
const Guideline* findGuideline(std::string_view id);

/// Rules that enforce a guideline, in the order the guide lists them.
const std::vector<std::string_view>& rulesForGuideline(std::string_view guidelineId);

/// Guidelines a rule enforces (empty when the rule is not part of the guide).
std::vector<std::string_view> guidelinesForRule(std::string_view ruleId);

/// True when `selector` names the guideline itself ("MG-4.1") or its chapter ("MG-4").
bool guidelineMatches(std::string_view selector, std::string_view guidelineId);

} // namespace uvmcheck::rules
