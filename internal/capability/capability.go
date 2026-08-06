// SPDX-License-Identifier: Apache-2.0

// Package capability is the Observation-and-Control Contract registry described
// in ADR-0001.
//
// Every ability VayuCell might have on a device is a registered Capability whose
// obligations are expressed as types with INVALID ZERO VALUES. A capability that
// leaves any obligation unanswered fails a test rather than a review, so nothing
// lands undeclared.
//
// Two rules here are load-bearing and are enforced by Complete rather than by
// convention:
//
//   - Verify may never be nil. A control that cannot be read back after being
//     set is indistinguishable from one that silently stopped working, and
//     reporting it would be the exact lie CHARTER Article IV forbids.
//
//   - A safety-class capability may not degrade quietly. Where a safety control
//     is absent it must refuse the operation, so the panel can render a
//     permanent failing row instead of a soft warning nobody reads.
package capability

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"strings"
)

// Tier is the environment a device provides, per ADR-0001 §1.
//
// A tier is DETECTED from positive evidence, never inferred from a model name
// or a version number. TierUnset is not a valid answer: a device whose tier
// could not be established is refused, not defaulted.
type Tier uint8

const (
	TierUnset Tier = iota // zero value — invalid, never a result
	T0                    // stock, unprivileged userspace
	T1                    // stock, root available
	T2                    // virtualised Linux guest
	T3                    // mainline Linux
)

var tierNames = map[Tier]string{T0: "T0", T1: "T1", T2: "T2", T3: "T3"}

func (t Tier) String() string {
	if n, ok := tierNames[t]; ok {
		return n
	}
	return "tierUnset"
}

// Valid reports whether t is a real tier rather than the zero value.
func (t Tier) Valid() bool { _, ok := tierNames[t]; return ok }

// AtLeast reports whether t satisfies a capability whose floor is min.
// An invalid tier satisfies nothing.
func (t Tier) AtLeast(min Tier) bool {
	if !t.Valid() || !min.Valid() {
		return false
	}
	return t >= min
}

// Class is what kind of thing a capability governs. It decides how strictly the
// registry treats absence.
type Class uint8

const (
	ClassUnset   Class = iota // zero value — invalid
	ClassSafety               // battery, thermal — never degrades quietly
	ClassServing              // ingress, site serving
	ClassStorage              // durability, wear
	ClassNetwork              // reachability, egress
	ClassObserve              // read-only measurement; may have no Apply
)

var classNames = map[Class]string{
	ClassSafety: "safety", ClassServing: "serving",
	ClassStorage: "storage", ClassNetwork: "network", ClassObserve: "observe",
}

func (c Class) String() string {
	if n, ok := classNames[c]; ok {
		return n
	}
	return "classUnset"
}

func (c Class) Valid() bool { _, ok := classNames[c]; return ok }

// Disposition is what happens when a capability is absent on this device.
type Disposition uint8

const (
	DispUnset   Disposition = iota // zero value — invalid
	DispDegrade                    // continue with reduced function
	DispRefuse                     // refuse the dependent operation
)

var dispNames = map[Disposition]string{DispDegrade: "degrade", DispRefuse: "refuse"}

func (d Disposition) String() string {
	if n, ok := dispNames[d]; ok {
		return n
	}
	return "dispUnset"
}

func (d Disposition) Valid() bool { _, ok := dispNames[d]; return ok }

// Result is the outcome of detecting or verifying a capability.
//
// There is deliberately no "ok" or "pass" member. Present means the mechanism
// exists; whether it is WORKING is a separate question answered by Verify.
// Unverified is never treated as Absent, and Absent is never treated as
// protection (CHARTER Article IV).
type Result uint8

const (
	ResultUnset Result = iota // zero value — invalid, never reported
	Present                   // mechanism found on this device
	Absent                    // mechanism genuinely not present
	Unverified                // could not be established — NOT the same as Absent
)

var resultNames = map[Result]string{Present: "present", Absent: "absent", Unverified: "unverified"}

func (r Result) String() string {
	if n, ok := resultNames[r]; ok {
		return n
	}
	return "resultUnset"
}

func (r Result) Valid() bool { _, ok := resultNames[r]; return ok }

// Observation is a Result with the evidence that produced it. The evidence is
// mandatory: a bare verdict with no reason cannot be audited, and an operator
// reading "absent" deserves to know what was looked for.
type Observation struct {
	Result   Result
	Evidence string // e.g. the node path that answered, or why nothing did
}

// ID identifies a capability. Stable across releases; it appears in the
// hardware database and in operator-facing reports.
type ID string

// DetectFn establishes whether the mechanism exists on this device.
type DetectFn func(ctx context.Context) (Observation, error)

// ApplyFn sets the capability to a target value. May be nil ONLY for
// ClassObserve capabilities, which measure rather than control.
type ApplyFn func(ctx context.Context, target string) error

// VerifyFn reads the state back from the hardware or kernel.
//
// This is the whole point of the package. It may never be nil.
type VerifyFn func(ctx context.Context) (Observation, error)

// Capability is one contract. Every field is an obligation.
type Capability struct {
	ID        ID
	Floor     Tier        // lowest tier that can provide this
	Class     Class       // what kind of thing this governs
	Detect    DetectFn    // is the mechanism here?
	Apply     ApplyFn     // set it (nil only when Class == ClassObserve)
	Verify    VerifyFn    // read it back — NEVER nil
	OnAbsent  Disposition // degrade, or refuse?
	Rationale string      // why these answers, in prose, shown to the operator
}

// Errors returned by Complete. Exported so tests can assert on the reason
// rather than on a string.
var (
	ErrNoID           = errors.New("capability: ID is empty")
	ErrFloorUnset     = errors.New("capability: Floor is tierUnset")
	ErrClassUnset     = errors.New("capability: Class is classUnset")
	ErrDispUnset      = errors.New("capability: OnAbsent is dispUnset")
	ErrNoDetect       = errors.New("capability: Detect is nil")
	ErrNoVerify       = errors.New("capability: Verify is nil — a control that cannot be read back may not be reported (CHARTER Art. IV)")
	ErrNoApply        = errors.New("capability: Apply is nil on a controlling capability (only ClassObserve may omit it)")
	ErrNoRationale    = errors.New("capability: Rationale is empty")
	ErrSafetyDegrades = errors.New("capability: a ClassSafety capability may not use DispDegrade — safety controls refuse or are reported permanently failing (CHARTER Art. III)")
)

// Complete reports whether every obligation was answered.
//
// It is called for every capability by a registry test, so a capability added
// without deciding its policy fails the build.
func (c Capability) Complete() error {
	var errs []error
	if strings.TrimSpace(string(c.ID)) == "" {
		errs = append(errs, ErrNoID)
	}
	if !c.Floor.Valid() {
		errs = append(errs, ErrFloorUnset)
	}
	if !c.Class.Valid() {
		errs = append(errs, ErrClassUnset)
	}
	if !c.OnAbsent.Valid() {
		errs = append(errs, ErrDispUnset)
	}
	if c.Detect == nil {
		errs = append(errs, ErrNoDetect)
	}
	if c.Verify == nil {
		errs = append(errs, ErrNoVerify)
	}
	if c.Apply == nil && c.Class != ClassObserve {
		errs = append(errs, ErrNoApply)
	}
	if strings.TrimSpace(c.Rationale) == "" {
		errs = append(errs, ErrNoRationale)
	}
	if c.Class == ClassSafety && c.OnAbsent == DispDegrade {
		errs = append(errs, ErrSafetyDegrades)
	}
	if len(errs) == 0 {
		return nil
	}
	return fmt.Errorf("capability %q: %w", c.ID, errors.Join(errs...))
}

// Registry holds every declared capability.
//
// Registration is fallible on purpose: an incomplete capability is refused at
// the point of registration rather than discovered at runtime.
type Registry struct {
	caps map[ID]Capability
}

// NewRegistry returns an empty registry.
func NewRegistry() *Registry { return &Registry{caps: make(map[ID]Capability)} }

// ErrDuplicate is returned when an ID is registered twice.
var ErrDuplicate = errors.New("capability: already registered")

// Register validates and adds a capability.
func (r *Registry) Register(c Capability) error {
	if err := c.Complete(); err != nil {
		return err
	}
	if _, dup := r.caps[c.ID]; dup {
		return fmt.Errorf("%w: %q", ErrDuplicate, c.ID)
	}
	r.caps[c.ID] = c
	return nil
}

// MustRegister panics if the capability is incomplete. Intended for package
// initialisation of compiled-in capabilities, where a failure is a build-time
// programming error rather than a runtime condition.
func (r *Registry) MustRegister(c Capability) {
	if err := r.Register(c); err != nil {
		panic(err)
	}
}

// Get returns a capability by ID.
func (r *Registry) Get(id ID) (Capability, bool) { c, ok := r.caps[id]; return c, ok }

// Len reports how many capabilities are registered.
func (r *Registry) Len() int { return len(r.caps) }

// IDs returns every registered ID, sorted, so reports are deterministic.
func (r *Registry) IDs() []ID {
	out := make([]ID, 0, len(r.caps))
	for id := range r.caps {
		out = append(out, id)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

// Validate re-checks every registered capability. Called by the registry test
// so that a capability mutated after registration still fails the build.
func (r *Registry) Validate() error {
	var errs []error
	for _, id := range r.IDs() {
		if err := r.caps[id].Complete(); err != nil {
			errs = append(errs, err)
		}
	}
	return errors.Join(errs...)
}
