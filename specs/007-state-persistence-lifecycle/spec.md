# Feature Specification: State Persistence & Lifecycle Management

**Feature Branch**: `007-state-persistence-lifecycle`  
**Created**: 2026-05-14  
**Status**: Draft  
**Input**: User description: "State Persistence & Lifecycle Management. The config and resume APIs exist but are decoupled from the application lifecycle. Hook application save persistence, serialize app configuration, flush current reading progress, load configuration and resume the last session at startup, and add a settings window for theme, zoom sensitivity, and reading direction."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Preserve Reading Progress on Exit (Priority: P1)

As a reader, I want the app to save my current book and page whenever the app exits or is suspended, so that forced exits, window closes, and normal shutdowns do not lose my place.

**Why this priority**: Losing reading progress is the highest-impact lifecycle failure because it directly damages trust in the reader.

**Independent Test**: Open a comic, navigate to a later page, trigger the app's save lifecycle, restart the app, and confirm the same comic and page are restored.

**Acceptance Scenarios**:

1. **Given** a comic is open on page 12, **When** the app save lifecycle runs, **Then** the user's progress records page 12 for that comic.
2. **Given** a comic is open and marked read by reaching the end, **When** the app save lifecycle runs, **Then** the saved progress preserves the read status.
3. **Given** the app is closed without the user manually returning to the library, **When** the app starts again, **Then** the user can continue from the last saved comic and page.

---

### User Story 2 - Resume Last Session on Startup (Priority: P2)

As a reader, I want the app to open directly to my last active reading session when one exists, so that I can continue reading without searching the library again.

**Why this priority**: Automatic resumption shortens the most common returning-user flow and validates that saved progress is useful.

**Independent Test**: Save progress for an available comic, restart the app, and verify the reader opens directly to that comic at the saved page.

**Acceptance Scenarios**:

1. **Given** a saved session exists for an available comic, **When** the app starts, **Then** the app enters the reader view for that comic and targets the saved page.
2. **Given** no saved session exists, **When** the app starts, **Then** the app opens to the library view.
3. **Given** the last saved comic is unavailable, **When** the app starts, **Then** the app opens to the library view and does not show a broken reader.

---

### User Story 3 - Adjust Reader Preferences (Priority: P3)

As a reader, I want a settings window reachable from the toolbar, so that I can change theme, zoom sensitivity, and default reading direction without editing files or restarting.

**Why this priority**: Preferences improve comfort and accessibility, but they depend on the persistence lifecycle from the higher-priority stories.

**Independent Test**: Open settings, change each preference, close and restart the app, and confirm the same preferences are active.

**Acceptance Scenarios**:

1. **Given** the app is open, **When** the user clicks the toolbar settings control, **Then** a settings window appears without interrupting the current reading or library context.
2. **Given** the settings window is open, **When** the user switches between dark and light appearance, **Then** the app updates appearance immediately and remembers the choice after restart.
3. **Given** the settings window is open, **When** the user changes zoom sensitivity, **Then** subsequent zoom gestures use the new sensitivity and the value persists after restart.
4. **Given** the settings window is open, **When** the user changes the default reading direction, **Then** new reading sessions use that direction and the value persists after restart.

### Edge Cases

- If settings cannot be saved, the app must continue running and surface a non-blocking warning instead of losing the current session.
- If saved configuration is missing or invalid, the app must fall back to safe defaults and remain usable.
- If saved progress points beyond the current page count, the resumed page must be clamped to the nearest valid page.
- If the previously active comic has been removed, moved, or marked unavailable, startup must route to the library and preserve enough state for the user to recover manually.
- If the app exits before a page fully loads, the most recent intended page index must be saved rather than reverting to an older page.
- Preference changes must not block the interface, interrupt active decoding, or reset the current reading position unless the changed preference requires a visual refresh.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The app MUST save the current reading session during its lifecycle save event when a comic is actively open.
- **FR-002**: Saved reading progress MUST include the active comic identity, current page, and read status.
- **FR-003**: The app MUST save user configuration during its lifecycle save event.
- **FR-004**: User configuration MUST include appearance preference, zoom sensitivity, and default reading direction.
- **FR-005**: On startup, the app MUST load saved user configuration before rendering the first view.
- **FR-006**: On startup, the app MUST attempt to resume the last valid saved reading session.
- **FR-007**: If a valid saved session exists for an available comic, the app MUST open directly into the reader view for that comic.
- **FR-008**: If no valid saved session exists, the app MUST open in the library view without showing an error dialog.
- **FR-009**: The app MUST provide a toolbar-accessible settings control in both library and reader contexts.
- **FR-010**: The settings window MUST allow users to change appearance preference, zoom sensitivity, and default reading direction.
- **FR-011**: Appearance preference changes MUST apply immediately while the app is running.
- **FR-012**: Preference changes MUST persist and be active after the app restarts.
- **FR-013**: The app MUST handle save and load failures without crashing or preventing normal reading.
- **FR-014**: The app MUST avoid duplicate or conflicting progress records when saving the same active session repeatedly.
- **FR-015**: The app MUST keep existing reading state intact when the settings window is opened, changed, or closed.

### Key Entities

- **App Configuration**: User preferences that affect app appearance and reader behavior, including appearance mode, zoom sensitivity, and default reading direction.
- **Reading Progress**: The user's saved position for a comic, including comic identity, current page, and whether the comic is read.
- **Last Session**: The most recent reading progress entry eligible for automatic resume.
- **Settings Window State**: Whether the settings window is visible and the currently edited preference values.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In 95% of normal close-and-restart trials, the app resumes to the same comic and page the user last had open.
- **SC-002**: Startup with a valid saved session reaches the reader view without user action in under 2 seconds for a typical local library.
- **SC-003**: Preference changes made through settings are reflected immediately in the current session and remain active in 100% of restart trials.
- **SC-004**: Invalid or missing saved settings never prevent the app from launching; users always reach either the library or a valid reader view.
- **SC-005**: Saving lifecycle state completes without visible interface stutter during ordinary reading and navigation.

## Assumptions

- A single local user profile is in scope; multi-user profile switching is out of scope.
- The last session is defined as the most recently saved active reading session.
- When a saved comic is unavailable, the app should prefer a safe library fallback over prompting during startup.
- Zoom sensitivity applies to future zoom gestures and does not retroactively alter the current zoom level.
- Default reading direction applies to newly opened or resumed reading sessions unless a future per-comic override is added.
