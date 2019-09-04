from deltachat import const
from conftest import wait_configuration_progress


def test_coi_capability(acfactory):
    ac = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac, 1000)

    assert ac.is_coi_supported()


def test_enabling_coi(acfactory):
    ac1 = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac1, 1000)

    ac1.set_coi_enabled(False, 1)
    ev1 = ac1._evlogger.get_matching("DC_EVENT_SET_METADATA_DONE")
    assert ev1[1] == 1
    assert ev1[2] == 0

    ac2 = acfactory.clone_online_account(ac1)
    wait_configuration_progress(ac2, 1000)

    assert not ac2.is_coi_enabled()

    ac2.set_coi_enabled(True, 2)
    ev2 = ac2._evlogger.get_matching("DC_EVENT_SET_METADATA_DONE")
    assert ev2[1] == 2
    assert ev2[2] == 0

    ac3 = acfactory.clone_online_account(ac1)
    wait_configuration_progress(ac3, 1000)

    assert ac3.is_coi_enabled()


def test_coi_message_filter(acfactory):
    ac1 = acfactory.get_online_configuring_account()
    wait_configuration_progress(ac1, 1000)

    ac1.set_coi_message_filter(const.DC_COI_FILTER_SEEN, 1)
    ev1 = ac1._evlogger.get_matching("DC_EVENT_SET_METADATA_DONE")
    assert ev1[1] == 1
    assert ev1[2] == 0

    ac2 = acfactory.clone_online_account(ac1)
    wait_configuration_progress(ac2, 1000)

    assert ac2.get_coi_message_filter() == const.DC_COI_FILTER_SEEN

    ac2.set_coi_message_filter(const.DC_COI_FILTER_ACTIVE, 2)
    ev2 = ac2._evlogger.get_matching("DC_EVENT_SET_METADATA_DONE")
    assert ev2[1] == 2
    assert ev2[2] == 0

    ac3 = acfactory.clone_online_account(ac1)
    wait_configuration_progress(ac3, 1000)

    assert ac3.get_coi_message_filter() == const.DC_COI_FILTER_ACTIVE
