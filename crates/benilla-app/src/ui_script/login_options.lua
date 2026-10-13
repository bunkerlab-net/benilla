-- Only this VM owns these adapters. The in-world Options implementation is unchanged.
assert(UIParent and BenillaOptionsFrame, "The game Options frame is unavailable")
UIParent:Hide()

local root = CreateFrame("Frame", "BenillaLoginOptionsRoot")
root:SetAllPoints()
root:SetFrameStrata("BACKGROUND")
local scene = CreateFrame("ModelFFX", "BenillaLoginOptionsScene")
scene:SetAllPoints()
scene:SetModel("Interface\\Glues\\Models\\UI_MainMenu\\UI_MainMenu.mdx")
scene:EnableMouse(false)

-- UIParent stays hidden to prevent the HUD and its OnUpdates from running. Reparent only
-- the existing Options window and its real shared dropdown, tooltip and dialog overlays.
BenillaOptionsFrame:SetParent(root)
BenillaOptionsFrame:ClearAllPoints()
BenillaOptionsFrame:SetPoint("CENTER", root, "CENTER", 0, 0)
for _, name in ipairs({"GameTooltip", "DropDownList1", "DropDownList2",
    "StaticPopup1", "StaticPopup2", "StaticPopup3", "StaticPopup4"}) do
    local frame = getglobal(name)
    if frame then frame:SetParent(root) end
end

-- These controls read/write a server-owned character record. There is no such record at
-- login, so show the real controls disabled with an explanation, including when Defaults runs.
BENILLA_LOGIN_CHARACTER_SETTING = "Available after entering the world with a character."
local locked = {}
for _, name in ipairs({
    "InterfaceRowShowTutorials", "InterfaceRowShowCloak", "InterfaceRowShowHelm",
    "ActionBarsRowMultiBar1", "ActionBarsRowMultiBar2", "ActionBarsRowMultiBar3",
    "ActionBarsRowMultiBar4"
}) do
    local row = getglobal("BenillaOptionsFrameContainerBody" .. name)
    if row then
        locked[row] = true
        row.tip = "BENILLA_LOGIN_CHARACTER_SETTING"
    end
end
local set = BenillaOptionsRow_Set
function BenillaOptionsRow_Set(row, value)
    if not locked[row] then set(row, value) end
end
local enabled = BenillaOptionsRow_SetEnabled
function BenillaOptionsRow_SetEnabled(row, on)
    if locked[row] then on = nil end
    enabled(row, on)
end
for row in pairs(locked) do BenillaOptionsRow_SetEnabled(row, nil) end

-- Account bindings have a real persistence destination before login. Character bindings do not.
BenillaKeyBindings_CharacterToggle = function() end
local character = BenillaOptionsFrameContainerBodyKeybindingsCharacterRowCheck
character:SetChecked(nil)
character:Disable()
BenillaOptionsFrameContainerBodyKeybindingsCharacterRowLabel:SetText("Character bindings (enter world first)")
character:SetScript("OnEnter", function()
    GameTooltip:SetOwner(this, "ANCHOR_RIGHT")
    GameTooltip:SetText(BENILLA_LOGIN_CHARACTER_SETTING, nil, nil, nil, nil, 1)
end)

-- Match UIParent's scale while retaining a full-screen unscaled scene behind the panel.
-- Scale changes and resizes are supported without recreating the VM or losing pending edits.
root:SetScript("OnUpdate", function()
    root:SetScale(UIParent:GetScale())
    BenillaOptionsFrame_UpdateScale()
end)
root:SetScale(UIParent:GetScale())
BenillaOptionsFrame:Show()
