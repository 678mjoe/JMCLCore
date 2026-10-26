"""In-memory source-guard controls; no Core, Java, process or runtime store calls."""
import unittest
import check_existing_candidate_proof as guard

class ReviewGuard(unittest.TestCase):
    def setUp(self):
        self.data = guard.sources()

    def test_reviewed_source(self):
        guard.validate(self.data)

    def test_crlf_checkout_is_equivalent(self):
        guard.validate({k:v.replace(b'\r\n', b'\n').replace(b'\n', b'\r\n')
                        for k,v in self.data.items()})

    def test_proof_change_rejected(self):
        self.data[guard.PROOF] += b'\nfn changed() {}\n'
        with self.assertRaisesRegex(ValueError, 'Proof changed'):
            guard.validate(self.data)

    def test_operational_change_rejected(self):
        self.data[guard.MANAGED] += b'\nfn changed() {}\n'
        with self.assertRaisesRegex(ValueError, 'Operational managed'):
            guard.validate(self.data)

    def test_non_test_wiring_rejected(self):
        self.data[guard.MANAGED] = self.data[guard.MANAGED].replace(b'#[cfg(test)]\n', b'', 1)
        with self.assertRaisesRegex(ValueError, 'cfg\\(test\\)'):
            guard.validate(self.data)

    def test_dependency_change_rejected(self):
        key = 'rust/src/java/receipt.rs'
        self.data[key] += b'\nfn changed() {}\n'
        with self.assertRaisesRegex(ValueError, 'Reviewed dependency changed'):
            guard.validate(self.data)

if __name__ == '__main__':
    unittest.main()
