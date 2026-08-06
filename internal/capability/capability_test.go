// SPDX-License-Identifier: Apache-2.0

package capability

import (
	"context"
	"errors"
	"testing"
)

// okDetect / okVerify are minimal valid function values for tests that are
// exercising a DIFFERENT obligation.
func okDetect(context.Context) (Observation, error) {
	return Observation{Result: Present, Evidence: "test"}, nil
}
func okVerify(context.Context) (Observation, error) {
	return Observation{Result: Present, Evidence: "test"}, nil
}
func okApply(context.Context, string) error { return nil }

// valid returns a capability that passes Complete, so each test can break
// exactly one thing.
func valid() Capability {
	return Capability{
		ID:        "test.capability",
		Floor:     T1,
		Class:     ClassServing,
		Detect:    okDetect,
		Apply:     okApply,
		Verify:    okVerify,
		OnAbsent:  DispRefuse,
		Rationale: "exists to exercise the registry",
	}
}

func TestCapabilityWithoutVerifyCannotBeRegistered(t *testing.T) {
	// The attack: ship a control that sets something and reports success on the
	// strength of the write returning no error. That is the transport-level lie
	// ADR-0002 §5 exists to prevent, and it must be impossible to express.
	c := valid()
	c.Verify = nil

	if err := c.Complete(); !errors.Is(err, ErrNoVerify) {
		t.Fatalf("a capability with no Verify must be refused; got %v", err)
	}
	if err := NewRegistry().Register(c); err == nil {
		t.Fatal("registry accepted a capability that cannot be read back")
	}
}

func TestSafetyCapabilityCannotDegradeQuietly(t *testing.T) {
	// The attack: register the battery charge ceiling with OnAbsent=DispDegrade
	// so a device that cannot limit charging keeps serving with a soft warning
	// nobody reads. CHARTER Article III forbids it.
	c := valid()
	c.Class = ClassSafety
	c.OnAbsent = DispDegrade

	if err := c.Complete(); !errors.Is(err, ErrSafetyDegrades) {
		t.Fatalf("a safety capability must not degrade quietly; got %v", err)
	}
	if err := NewRegistry().Register(c); err == nil {
		t.Fatal("registry accepted a silently-degrading safety capability")
	}
}

func TestSafetyCapabilityMayRefuse(t *testing.T) {
	// The corollary: refusing IS allowed, otherwise the rule above would make
	// safety capabilities unregisterable rather than strict.
	c := valid()
	c.Class = ClassSafety
	c.OnAbsent = DispRefuse

	if err := c.Complete(); err != nil {
		t.Fatalf("a refusing safety capability must be registerable: %v", err)
	}
}

func TestEveryZeroValuedObligationIsRefused(t *testing.T) {
	// Each obligation's zero value must be invalid on its own. Table-driven so
	// adding a field without a zero-value check fails here.
	cases := []struct {
		name    string
		breakIt func(*Capability)
		want    error
	}{
		{"no ID", func(c *Capability) { c.ID = "" }, ErrNoID},
		{"blank ID", func(c *Capability) { c.ID = "   " }, ErrNoID},
		{"tier unset", func(c *Capability) { c.Floor = TierUnset }, ErrFloorUnset},
		{"class unset", func(c *Capability) { c.Class = ClassUnset }, ErrClassUnset},
		{"disposition unset", func(c *Capability) { c.OnAbsent = DispUnset }, ErrDispUnset},
		{"no Detect", func(c *Capability) { c.Detect = nil }, ErrNoDetect},
		{"no Verify", func(c *Capability) { c.Verify = nil }, ErrNoVerify},
		{"no Rationale", func(c *Capability) { c.Rationale = "" }, ErrNoRationale},
		{"blank Rationale", func(c *Capability) { c.Rationale = "  " }, ErrNoRationale},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			c := valid()
			tc.breakIt(&c)
			if err := c.Complete(); !errors.Is(err, tc.want) {
				t.Fatalf("want %v, got %v", tc.want, err)
			}
		})
	}
}

func TestOnlyObserveCapabilitiesMayOmitApply(t *testing.T) {
	// A controlling capability with no Apply is a report pretending to be a
	// control. Observe-only capabilities legitimately measure without setting.
	c := valid()
	c.Apply = nil
	if err := c.Complete(); !errors.Is(err, ErrNoApply) {
		t.Fatalf("a controlling capability needs Apply; got %v", err)
	}

	c.Class = ClassObserve
	if err := c.Complete(); err != nil {
		t.Fatalf("an observe capability may omit Apply: %v", err)
	}
	// ...but it still may not omit Verify.
	c.Verify = nil
	if err := c.Complete(); !errors.Is(err, ErrNoVerify) {
		t.Fatalf("observe capabilities still require Verify; got %v", err)
	}
}

func TestUnverifiedIsNeverAbsentAndNeitherIsProtection(t *testing.T) {
	// CHARTER Article IV: absence is never protection, and what cannot be
	// checked reads unverified rather than clean. These are distinct values and
	// must never compare equal.
	if Unverified == Absent {
		t.Fatal("Unverified and Absent must be distinct results")
	}
	if ResultUnset.Valid() {
		t.Fatal("ResultUnset must be invalid")
	}
	for _, r := range []Result{Present, Absent, Unverified} {
		if !r.Valid() {
			t.Fatalf("%v must be a valid result", r)
		}
	}
	if ResultUnset.String() != "resultUnset" {
		t.Fatalf("the zero result must name itself as unset, got %q", ResultUnset.String())
	}
}

func TestTierIsNeverSatisfiedByTheZeroValue(t *testing.T) {
	// The attack: a device whose tier could not be established gets treated as
	// satisfying a T0 floor, silently granting capabilities to hardware nobody
	// probed. ADR-0001 §2: detection is positive evidence only.
	if TierUnset.AtLeast(T0) {
		t.Fatal("an undetected tier must satisfy nothing")
	}
	if T1.AtLeast(TierUnset) {
		t.Fatal("an unset floor must be satisfiable by nothing")
	}
	if !T2.AtLeast(T1) {
		t.Fatal("a higher tier must satisfy a lower floor")
	}
	if T0.AtLeast(T3) {
		t.Fatal("a lower tier must not satisfy a higher floor")
	}
}

func TestRegistryRefusesDuplicates(t *testing.T) {
	r := NewRegistry()
	if err := r.Register(valid()); err != nil {
		t.Fatalf("first registration should succeed: %v", err)
	}
	if err := r.Register(valid()); !errors.Is(err, ErrDuplicate) {
		t.Fatalf("want ErrDuplicate, got %v", err)
	}
	if r.Len() != 1 {
		t.Fatalf("duplicate must not be stored; len=%d", r.Len())
	}
}

func TestValidateCatchesMutationAfterRegistration(t *testing.T) {
	// A capability that passed Complete at registration but was mutated
	// afterwards must still fail the build via the registry test.
	r := NewRegistry()
	if err := r.Register(valid()); err != nil {
		t.Fatal(err)
	}
	c := r.caps["test.capability"]
	c.Verify = nil
	r.caps["test.capability"] = c

	if err := r.Validate(); !errors.Is(err, ErrNoVerify) {
		t.Fatalf("Validate must catch a mutated capability; got %v", err)
	}
}

func TestIDsAreSortedSoReportsAreDeterministic(t *testing.T) {
	r := NewRegistry()
	for _, id := range []ID{"z.last", "a.first", "m.middle"} {
		c := valid()
		c.ID = id
		if err := r.Register(c); err != nil {
			t.Fatal(err)
		}
	}
	got := r.IDs()
	want := []ID{"a.first", "m.middle", "z.last"}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("IDs not sorted: got %v want %v", got, want)
		}
	}
}
