from conftest import wait_configuration_progress

def test_webpush_capability(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    assert ac.is_webpush_supported()

def test_webpush_vapid_key(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    assert ac.get_webpush_vapid_key() is not None

def test_webpush_subscription(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    uid = "test_webpush_subscription"
    sub = {
        "client": "DCC integration test",
        "device": "PC",
        "msgtype": "chat"
    }

    assert ac.subscribe_webpush(uid, sub)
    assert ac.get_webpush_subscription(uid) == sub

    assert ac.subscribe_webpush(uid, None)
    assert ac.get_webpush_subscription(uid) is None
