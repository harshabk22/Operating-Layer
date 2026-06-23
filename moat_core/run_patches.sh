#!/bin/bash
python3 patch_api.py || echo "patch_api failed"
python3 patch_bookings.py || echo "patch_bookings failed"
python3 src/api/patch_css.py || echo "patch_css failed"
python3 src/api/patch_js.py || echo "patch_js failed"
python3 src/api/fix_dashboard.py || echo "fix_dashboard failed"
python3 src/api/fix_badges.py || echo "fix_badges failed"
python3 src/api/fix_syntax.py || echo "fix_syntax failed"
