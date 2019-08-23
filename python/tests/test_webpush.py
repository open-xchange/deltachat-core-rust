from conftest import wait_configuration_progress

def test_webpush_capability(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    assert ac.is_webpush_supported()
