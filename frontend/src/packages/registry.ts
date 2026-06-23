import React from 'react';
import Leads from './sales-core-v1/pages/Leads';
import Opportunities from './sales-core-v1/pages/Opportunities';
import Properties from './real-estate-v1/pages/Properties';
import Offers from './real-estate-v1/pages/Offers';
import Deals from './real-estate-v1/pages/Deals';
import Notifications from './notification-package-v1/pages/Notifications';
import Templates from './notification-package-v1/pages/Templates';
import History from './notification-package-v1/pages/History';

export const componentMap: Record<string, React.ComponentType> = {
  Leads,
  Opportunities,
  Properties,
  Offers,
  Deals,
  Notifications,
  Templates,
  History,
};
