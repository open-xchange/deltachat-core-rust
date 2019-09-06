//
//  ContextCallHandler.swift
//  delta_chat_core
//
//  Created by Cihan Oezkan on 12.08.19.
//

import Foundation

extension CallHandler {
    
    public func handleContextCalls(methodCall: FlutterMethodCall, result: FlutterResult) {
        switch (methodCall.method) {
            case Method.Context.CONFIG_SET:
                setConfig(methodCall: methodCall, result: result)
                break
            case Method.Context.CONFIG_GET:
                getConfig(methodCall: methodCall, result: result, type: ArgumentType.STRING)
                break
            case Method.Context.CONFIG_GET_INT:
                getConfig(methodCall: methodCall, result: result, type: ArgumentType.INT)
                break
            case Method.Context.CONFIGURE:
                configure(result: result)
                break
            case Method.Context.IS_CONFIGURED:
                let isConfigured: Bool = dc_is_configured(mailboxPointer) == 1
                result(isConfigured)
                break
            case Method.Context.ADD_ADDRESS_BOOK:
                addAddressBook(methodCall: methodCall, result: result)
                break
            case Method.Context.CREATE_CONTACT:
                createContact(methodCall: methodCall, result: result)
                break
            case Method.Context.DELETE_CONTACT:
                deleteContact(methodCall: methodCall, result: result)
                break
            case Method.Context.BLOCK_CONTACT:
                blockContact(methodCall: methodCall, result: result)
                break
            case Method.Context.UNBLOCK_CONTACT:
                unblockContact(methodCall: methodCall, result: result)
                break
            case Method.Context.CREATE_CHAT_BY_CONTACT_ID:
                createChatByContactId(methodCall: methodCall, result: result)
                break
            case Method.Context.CREATE_CHAT_BY_MESSAGE_ID:
                createChatByMessageId(methodCall: methodCall, result: result)
                break
            case Method.Context.CREATE_GROUP_CHAT:
                createGroupChat(methodCall: methodCall, result: result)
                break
            case Method.Context.GET_CONTACT:
                getContact(methodCall: methodCall, result: result)
                break
            case Method.Context.GET_CONTACTS:
                getContacts(methodCall: methodCall, result: result)
                break
            case Method.Context.GET_CHAT_CONTACTS:
                getChatContacts(methodCall: methodCall, result: result)
                break
            case Method.Context.GET_CHAT:
                getChat(methodCall: methodCall, result: result)
                break
            case Method.Context.GET_CHAT_MESSAGES:
                getChatMessages(methodCall: methodCall, result: result)
                break
            case Method.Context.CREATE_CHAT_MESSAGE:
                createChatMessage(methodCall: methodCall, result: result)
                break
            case Method.Context.CREATE_CHAT_ATTACHMENT_MESSAGE:
                createChatAttachmentMessage(methodCall: methodCall, result: result)
                break
            case Method.Context.ADD_CONTACT_TO_CHAT:
                addContactToChat(methodCall: methodCall, result: result)
                break
            case Method.Context.GET_CHAT_BY_CONTACT_ID:
                getChatByContactId(methodCall: methodCall, result: result)
                break
            case Method.Context.GET_BLOCKED_CONTACTS:
                getBlockedContacts(result: result)
                break
            case Method.Context.GET_FRESH_MESSAGE_COUNT:
                getFreshMessageCount(methodCall: methodCall, result: result)
                break
            case Method.Context.MARK_NOTICED_CHAT:
                markNoticedChat(methodCall: methodCall, result: result)
                break
            case Method.Context.DELETE_CHAT:
                deleteChat(methodCall: methodCall, result: result)
                break
            case Method.Context.REMOVE_CONTACT_FROM_CHAT:
                removeContactFromChat(methodCall: methodCall, result: result)
                break
            case Method.Context.EXPORT_KEYS:
                //exportImportKeys(methodCall, result, DcContext.DC_IMEX_EXPORT_SELF_KEYS)
                break
            case Method.Context.IMPORT_KEYS:
                //exportImportKeys(methodCall, result, DcContext.DC_IMEX_IMPORT_SELF_KEYS)
                break
            case Method.Context.GET_FRESH_MESSAGES:
                getFreshMessages(result: result)
                break
            case Method.Context.FORWARD_MESSAGES:
                forwardMessages(methodCall: methodCall, result: result)
                break
            case Method.Context.MARK_SEEN_MESSAGES:
                markSeenMessages(methodCall: methodCall, result: result)
                break
            case Method.Context.INITIATE_KEY_TRANSFER:
                initiateKeyTransfer(result: result)
                break
            case Method.Context.CONTINUE_KEY_TRANSFER:
                continueKeyTransfer(methodCall: methodCall, result: result)
            case Method.Context.GET_SECUREJOIN_QR:
                getSecurejoinQr(methodCall: methodCall, result: result)
                break
            case Method.Context.JOIN_SECUREJOIN:
                joinSecurejoin(methodCall: methodCall, result: result)
                break
            case Method.Context.CHECK_QR:
                checkQr(methodCall: methodCall, result: result)
                break
            case Method.Context.STOP_ONGOING_PROCESS:
                dc_stop_ongoing_process(mailboxPointer)
                result(nil)
                break
            case Method.Context.DELETE_MESSAGES:
                deleteMessages(methodCall: methodCall, result: result)
            case Method.Context.STAR_MESSAGES:
                starMessages(methodCall: methodCall, result: result)
                break
            case Method.Context.SET_CHAT_NAME:
                setChatName(methodCall: methodCall, result: result)
                break
            case Method.Context.SET_CHAT_PROFILE_IMAGE:
                setChatProfileImage(methodCall: methodCall, result: result)
                break
            default:
                print("Context: Failing for \(methodCall.method)")
                _ = FlutterMethodNotImplemented
        }
    }
    
    private func setConfig(methodCall: FlutterMethodCall, result: FlutterResult) {
        let parameters = methodCall.parameters

        if let key = parameters["key"] as? String,
            let value = parameters["value"] as? String {
            
            let dc_result = dcConfig.set(value: value, for: key)
            result(NSNumber(value: dc_result))
        }
        else {
            result("iOS could not extract flutter arguments in method: (sendParams)")
        }
    }
    
    private func getConfig(methodCall: FlutterMethodCall, result: FlutterResult, type: String) {
        let parameters = methodCall.parameters
        
        if let key = parameters["key"] as? String {
            
            switch (type) {
                case ArgumentType.STRING:
                    let value = dc_get_config(mailboxPointer, key)
                    if let pSafe = value {
                        let c = String(cString: pSafe)
                        if c.isEmpty {
                            result(nil)
                        }
                        result(c)
                    }
                    break
                
                case ArgumentType.INT:
                    let value = dc_get_config(mailboxPointer, key)
                    if let pSafe = value {
                        let c = Int(bitPattern: pSafe)
                        
                        result(c)
                    }
                    break
                
                default: break
            }
            
        } else {
            result("iOS could not extract flutter arguments in method: (sendParams)")
        }
    }
    
    private func configure(result: FlutterResult) {
        result(dc_configure(mailboxPointer));
    }
    
    private func addAddressBook(methodCall: FlutterMethodCall, result: FlutterResult) {
        if !methodCall.contains(keys: [Argument.ADDRESS_BOOK]) {
            Method.errorMissingArgument(result: result);
            return
        }

        let parameters = methodCall.parameters
        
        if let addressBook = parameters[Argument.ADDRESS_BOOK] as? String {
            
            let dc_result = dc_add_address_book(mailboxPointer, addressBook)
            result(dc_result)
        }
        else {
            Method.errorArgumentMissingValue(result: result)
            return
        }
    
    }
    
    private func createContact(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            return
        }
        if let myArgs = args as? [String: Any],
            let address = myArgs["address"] as? String {
            
            let name = myArgs["name"] as? String
            let contactId = dc_create_contact(mailboxPointer, name, address)
            result(Int(contactId))
        }
        else {
            result(Method.errorArgumentMissingValue(result: result))
        }
    }
    
    private func deleteContact(methodCall: FlutterMethodCall, result: FlutterResult) {
        if !methodCall.contains(keys: [Argument.ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        guard let args = methodCall.arguments else {
            return
        }

        if let myArgs = args as? [String: Any],
            let contactId = myArgs[Argument.ID] as? UInt32 {
            
            let deleted = dc_delete_contact(mailboxPointer, contactId)
            result(deleted)
        }
        else {
            Method.errorArgumentMissingValue(result: result)
            return
        }
    
    }
    
    private func blockContact(methodCall: FlutterMethodCall, result: FlutterResult) {
        if !methodCall.contains(keys: [Argument.ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        guard let args = methodCall.arguments else {
            return
        }

        if let myArgs = args as? [String: Any],
            let contactId = myArgs[Argument.ID] as? UInt32 {
            
            dc_block_contact(mailboxPointer, contactId, 1)
            result(nil)
        }
        else {
            Method.errorArgumentMissingValue(result: result)
            return
        }
    
    }
    
    private func getBlockedContacts(result: FlutterResult) {
        let blockedIds = dc_get_blocked_contacts(mailboxPointer)
        result(blockedIds)
    
    }
    
    private func unblockContact(methodCall: FlutterMethodCall, result: FlutterResult) {
        if !methodCall.contains(keys: [Argument.ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        guard let args = methodCall.arguments else {
            return
        }
        
        if let myArgs = args as? [String: Any], let contactId = myArgs[Argument.ID] as? UInt32 {
            dc_block_contact(mailboxPointer, contactId, 0)
            result(nil)
        }
        else {
            Method.errorArgumentMissingValue(result: result)
            return
        }
    
    }
    
    private func createChatByContactId(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            return
        }
        
        if let myArgs = args as? [String: Any],
            let contactId = myArgs[Argument.ID] as? UInt32 {
            
            let chatId = dc_create_chat_by_contact_id(mailboxPointer, contactId)
            result(Int(chatId))
        }
        else {
            result(Method.errorMissingArgument(result: result))
        }
    }
    
    private func createChatByMessageId(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            return
        }
        
        if let myArgs = args as? [String: Any],
            let messageId = myArgs[Argument.ID] as? UInt32 {
            
            let chatId = dc_create_chat_by_msg_id(mailboxPointer, messageId)
            result(Int(chatId))
        }
        else {
            result(Method.errorMissingArgument(result: result))
        }
    
    }
    
    private func createGroupChat(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.VERIFIED, Argument.NAME]) {
            result(Method.errorMissingArgument(result: result))
            return
        }

        if let myArgs = args as? [String: Any] {
            let verified = myArgs[Argument.VERIFIED] as? Bool
            if (verified == nil) {
                result(Method.errorMissingArgument(result: result))
                return
            }
            
            let name = myArgs[Argument.NAME] as? String
            let chatId = dc_create_group_chat(mailboxPointer, Int32(truncating: verified! as NSNumber), name)
            
            result(chatId)
        }
    }
    
    private func addContactToChat(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID, Argument.CONTACT_ID]) {
            result(Method.errorMissingArgument(result: result))
            return
        }

        if let myArgs = args as? [String: Any] {
            let chatId = myArgs[Argument.CHAT_ID] as? UInt32
            let contactId = myArgs[Argument.CONTACT_ID] as? UInt32
            
            if (chatId == nil || contactId == nil) {
                result(Method.errorMissingArgument(result: result))
                return
            }
            
            let successfullyAdded = dc_add_contact_to_chat(mailboxPointer, chatId!, contactId!)
            
            result(successfullyAdded)
        }
        
    }
    
    private func getChatByContactId(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.CONTACT_ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let contactId = myArgs[Argument.CONTACT_ID] as? UInt32 {
            let chatId = dc_get_chat_id_by_contact_id(mailboxPointer, contactId)
            result(chatId)
        }
        else {
            Method.errorArgumentMissingValue(result: result);
            return
        }
    
    }
    
    private func getContact(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result)
            return
        }
        if let myArgs = args as? [String: Any], let id = myArgs[Argument.ID] as? UInt32 {
            result(dc_array_get_contact_id(mailboxPointer, Int(id)))
        }
        
    }
    
    private func getContacts(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.FLAGS, Argument.QUERY]) {
            result(Method.errorMissingArgument(result: result))
            return
        }

        if let myArgs = args as? [String: Any] {
            let flags = myArgs[Argument.FLAGS] as? UInt32
            let query = myArgs[Argument.QUERY]  as? String
            
            if (flags == nil) {
                result(Method.errorMissingArgument(result: result))
                return
            }
            
            var contactIds = dc_get_contacts(mailboxPointer, flags!, query)
            
            result(FlutterStandardTypedData(int32: Data(bytes: &contactIds, count: MemoryLayout.size(ofValue: contactIds))))
        }
        
    }
    
    private func getChatContacts(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID]) {
            result(Method.errorMissingArgument(result: result))
            return
        }

        if let myArgs = args as? [String: Any] {
            let id = myArgs[Argument.CHAT_ID] as? UInt32
            
            if id == nil {
                result(Method.errorMissingArgument(result: result))
                return
            }
            
            var contactIds = dc_get_chat_contacts(mailboxPointer, id!)
            
            result(FlutterStandardTypedData(int32: Data(bytes: &contactIds, count: MemoryLayout.size(ofValue: contactIds))))
        }
        
    }
    
    private func getChat(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result)
            return
        }
        
        if !methodCall.contains(keys: [Argument.ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let id = myArgs[Argument.ID] as? UInt32 {
            
            result(dc_get_chat(mailboxPointer, id))
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    }
    
    private func getChatMessages(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result)
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let chatId = myArgs[Argument.CHAT_ID] as? UInt32,
            let flags = myArgs[Argument.FLAGS] {
            //result(dc_get_chat(mailboxPointer, chatId))
        }
        else {
            Method.errorMissingArgument(result: result);
        }
//        Integer id = methodCall.argument(ARGUMENT_CHAT_ID);
//        if (id == null) {
//        resultErrorArgumentMissingValue(result);
//        return;
//        }
//        Integer flags = methodCall.argument(ARGUMENT_FLAGS);
//        if (flags == null) {
//        flags = 0;
//        }
//
//        int[] messageIds = dcContext.getChatMsgs(id, flags, 0);
//        for (int messageId : messageIds) {
//        if (messageId != DcMsg.DC_MSG_ID_MARKER1 && messageId != DcMsg.DC_MSG_ID_DAYMARKER) {
//        DcMsg message = messageCache.get(messageId);
//        if (message == null) {
//        messageCache.put(messageId, dcContext.getMsg(messageId));
//        }
//        }
//        }
//        result.success(messageIds);
    }
    
    private func createChatMessage(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result)
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID, Argument.TEXT]) {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if let myArgs = args as? [String: Any],
            let chatId = myArgs[Argument.CHAT_ID] as? UInt32,
            let flags = myArgs[Argument.TEXT] {
            //result(dc_get_chat(mailboxPointer, chatId))
            
        }
        else {
            Method.errorMissingArgument(result: result);
        }
        
//    Integer id = methodCall.argument(ARGUMENT_CHAT_ID);
//    if (id == null) {
//    resultErrorArgumentMissingValue(result);
//    return;
//    }
//    
//    String text = methodCall.argument(ARGUMENT_TEXT);
//    
//    DcMsg newMsg = new DcMsg(dcContext, DcMsg.DC_MSG_TEXT);
//    newMsg.setText(text);
//    int messageId = dcContext.sendMsg(id, newMsg);
//    result.success(messageId);
    }
    
    private func createChatAttachmentMessage(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result)
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID, Argument.TYPE, Argument.PATH, Argument.TEXT]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let chatId = myArgs[Argument.CHAT_ID] as? UInt32,
            let path = myArgs[Argument.PATH],
            let type = myArgs[Argument.TYPE] {
            
            let text = myArgs[Argument.TEXT]
            
        }
        else {
            Method.errorMissingArgument(result: result);
        }
        
//    Integer chatId = methodCall.argument(ARGUMENT_CHAT_ID);
//    String path = methodCall.argument(ARGUMENT_PATH);
//    Integer type = methodCall.argument(ARGUMENT_TYPE);
//    if (chatId == null || path == null || type == null) {
//    resultErrorArgumentMissingValue(result);
//    return;
//    }
//
//    String text = methodCall.argument(ARGUMENT_TEXT);
//
//    DcMsg newMsg = new DcMsg(dcContext, type);
//    newMsg.setFile(path, null);
//    newMsg.setText(text);
//    int messageId = dcContext.sendMsg(chatId, newMsg);
//    result.success(messageId);
    }
    
    private func getFreshMessageCount(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID]) {
            result(Method.errorMissingArgument(result: result))
            return
        }

        if let myArgs = args as? [String: Any] {
            let chatId = myArgs[Argument.CHAT_ID] as? UInt32

            if chatId == nil {
                result(Method.errorMissingArgument(result: result))
                return
            }

            let freshMessageCount = dc_get_fresh_msg_cnt(mailboxPointer, chatId!)

            result(freshMessageCount)
        }
        
    }
    
    private func markNoticedChat(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID]) {
            result(Method.errorMissingArgument(result: result))
            return
        }

        if let myArgs = args as? [String: Any] {
            let chatId = myArgs[Argument.CHAT_ID] as? UInt32

            if chatId == nil {
                result(Method.errorMissingArgument(result: result))
                return
            }

            dc_marknoticed_chat(mailboxPointer, chatId!)
            result(nil)
        }
        
    }
    
    private func deleteChat(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            fatalError()
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID]) {
            result(Method.errorMissingArgument(result: result))
            return
        }

        if let myArgs = args as? [String: Any] {
            let chatId = myArgs[Argument.CHAT_ID] as? UInt32

            if chatId == nil {
                result(Method.errorMissingArgument(result: result))
                return
            }

            dc_delete_chat(mailboxPointer, chatId!)
            result(nil)
        }
        
    }
    
    private func removeContactFromChat(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID, Argument.CONTACT_ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any], let chatId = myArgs[Argument.CHAT_ID], let contactId = myArgs[Argument.CONTACT_ID] {
            result(dc_remove_contact_from_chat(mailboxPointer, chatId as! UInt32, contactId as! UInt32))
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    
    }
    
    private func getFreshMessages(result: FlutterResult) {
        let freshMessages = dc_get_fresh_msgs(mailboxPointer)
        result(freshMessages)
    }
    
    private func forwardMessages(methodCall: FlutterMethodCall, result: FlutterResult){
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID, Argument.MESSAGE_IDS]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any], let chatId = myArgs[Argument.CHAT_ID], let msgIdArray = myArgs[Argument.MESSAGE_IDS] {
            
            result(dc_forward_msgs(mailboxPointer, msgIdArray as? UnsafePointer<UInt32>, Int32((msgIdArray as AnyObject).count), chatId as! UInt32))
        }
        else {
            Method.errorMissingArgument(result: result);
        }

    }
    
    private func markSeenMessages(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.MESSAGE_IDS]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any], let msgIdArray = myArgs[Argument.MESSAGE_IDS] {
            result(dc_markseen_msgs(mailboxPointer, msgIdArray as? UnsafePointer<UInt32>, Int32((msgIdArray as AnyObject).count)))
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    
    }
    
    private func initiateKeyTransfer(result: FlutterResult) {
//    new Thread(() -> {
//    String setupKey = dcContext.initiateKeyTransfer();
//    getUiThreadHandler().post(() -> {
//    result.success(setupKey);
//    });
//    }).start();
    }
    
    private func continueKeyTransfer(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.ID, Argument.SETUP_CODE]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let messageId = myArgs[Argument.ID],
            let setupCode = myArgs[Argument.SETUP_CODE] {
            
            result(dc_continue_key_transfer(mailboxPointer, messageId as! UInt32, setupCode as? UnsafePointer<Int8>))
        }
        else {
            Method.errorMissingArgument(result: result);
        }
        
    }
    
    private func getSecurejoinQr(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let chatId = myArgs[Argument.CHAT_ID] {
            result(dc_get_securejoin_qr(mailboxPointer, chatId as! UInt32))
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    
    }
    
    private func  joinSecurejoin(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.QR_TEXT]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let qrText = myArgs[Argument.QR_TEXT] {
//            new Thread(() -> {
//                int chatId = dcContext.joinSecurejoin(qrText);
//                getUiThreadHandler().post(() -> {
//                    result.success(chatId);
//                    });
//                }).start();
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    
    }
    
    private func checkQr(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.QR_TEXT]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let qrText = myArgs[Argument.QR_TEXT] {
//            DcLot qrCode = dcContext.checkQr(qrText);
//            result.success(mapLotToList(qrCode));
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    
    }
    
    private func deleteMessages(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.MESSAGE_IDS]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let msgIdArray = myArgs[Argument.MESSAGE_IDS] {
            result(dc_delete_msgs(mailboxPointer, msgIdArray as? UnsafePointer<UInt32>, Int32((msgIdArray as AnyObject).count)))
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    
    }
    
    private func starMessages(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let arguments: [String: Any] = methodCall.arguments as? [String: Any] else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.MESSAGE_IDS, Argument.VALUE]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let msgIds = arguments[Argument.MESSAGE_IDS] {
            let star = methodCall.intValue(for: Argument.VALUE, result: result)

            dc_star_msgs(mailboxPointer, msgIds as? UnsafePointer<UInt32>, Int32((msgIds as AnyObject).count), Int32(star))
            result(nil)
        }
        else {
            Method.errorMissingArgument(result: result);
        }
    
    }
    
    private func setChatProfileImage(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID, Argument.VALUE]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let chatId = myArgs[Argument.CHAT_ID],
            let value = myArgs[Argument.VALUE] {
            
            dc_set_chat_profile_image(mailboxPointer, chatId as! UInt32, value as? UnsafePointer<Int8>)
            result(nil)
        }
        else {
            Method.errorMissingArgument(result: result)
            return
        }
    
    }
    
    private func setChatName(methodCall: FlutterMethodCall, result: FlutterResult) {
        guard let args = methodCall.arguments else {
            Method.errorMissingArgument(result: result);
            return
        }
        
        if !methodCall.contains(keys: [Argument.CHAT_ID, Argument.VALUE]) {
            Method.errorMissingArgument(result: result);
            return
        }

        if let myArgs = args as? [String: Any],
            let chatId = myArgs[Argument.CHAT_ID],
            let value = myArgs[Argument.VALUE] {
            
            dc_set_chat_name(mailboxPointer, chatId as! UInt32, value as? UnsafePointer<Int8>)
            result(nil)
        }
        else {
            Method.errorMissingArgument(result: result)
            return
        }
    
    }
    
}
