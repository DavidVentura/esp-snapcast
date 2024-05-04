import sys
import os
ssid = b"J" * 31 + b"\0"
pwd = b"K" * 63 + b"\0"
null = b"\0"

new_ssid = os.environ.get("SSID")
new_pwd = os.environ.get("PASS")

if not new_ssid or not new_pwd:
    print("You must set the 'SSID' and 'PASS' environment variables when generating this firmware file")
    sys.exit(1)

new_ssid = new_ssid.encode()
new_pwd = new_pwd.encode()

with open(sys.argv[1], "rb+") as fd:
    fdata = fd.read()

    new_ssid = new_ssid + (len(ssid) - len(new_ssid)) * null
    new_pwd = new_pwd + (len(pwd) - len(new_pwd)) * null

    assert len(new_ssid) == len(ssid)
    assert len(new_pwd) == len(pwd)

    new_fdata = fdata.replace(ssid, new_ssid).replace(pwd, new_pwd)
    assert new_ssid in new_fdata
    assert new_pwd in new_fdata
    assert ssid not in new_fdata
    assert pwd not in new_fdata
    fd.seek(0)
    fd.write(new_fdata)
