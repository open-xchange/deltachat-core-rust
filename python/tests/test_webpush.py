import json
import re
from conftest import wait_configuration_progress

def test_webpush_capability(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    assert ac.is_webpush_supported()

def test_webpush_vapid_key(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    vapid = ac.get_webpush_vapid_key()
    assert re.match(r"-----BEGIN PUBLIC KEY-----\r?\n[A-Za-z0-9+/\r\n]+={,3}\r?\n-----END PUBLIC KEY-----", vapid)

def test_webpush_subscription(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    uid = "test_webpush_subscription"
    sub = {
        "client": "Test Client",
        "device": "Test Phone",
        "msgtype": "chat",
        "resource": {
            "endpoint": "http://localhost/",
            "keys": {
                "p256dh": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "auth": "1234567890123456"
            }
        }
    }

    ac.subscribe_webpush(uid, sub, 1)
    ev1 = ac._evlogger.get_matching("DC_EVENT_SET_METADATA_DONE")
    assert ev1[1] == 1
    assert ev1[2] == 0

    ac.get_webpush_subscription(uid, 2)
    ev2 = ac._evlogger.get_matching("DC_EVENT_WEBPUSH_SUBSCRIPTION")
    assert ev2[1] == 2
    assert ev2[2] != 0 and json.loads(ev2[2]) == sub

    ac.subscribe_webpush(uid, None, 3)
    ev3 = ac._evlogger.get_matching("DC_EVENT_SET_METADATA_DONE")
    assert ev3[1] == 3
    assert ev3[2] == 0

    ac.get_webpush_subscription(uid, 4)
    ev4 = ac._evlogger.get_matching("DC_EVENT_WEBPUSH_SUBSCRIPTION")
    assert ev4[1] == 4
    assert ev4[2] == 0
